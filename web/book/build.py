#!/usr/bin/env python3
"""Build the Science book: web/book/src/*.md -> web/book/*.html.

    python3 web/book/build.py            # build
    python3 web/book/build.py --check    # build, then run every example

Standard library only. The chapter order and the sidebar both come from
SUMMARY.md, so adding a chapter is one line there and one file in src/.

Code fences:

    ```science            checked with `sciencec check`, must pass
    ```science run        built and run with `sciencec run`; the ```output
                          fence right after it must match stdout exactly
    ```science fails      must be refused by `sciencec check`
    ```science norun      shown only, not checked (a fragment)
    ```shell / ```output / ```text   shown as is
"""

import html
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "src")
ROOT = os.path.dirname(os.path.dirname(HERE))
SCIENCEC = os.environ.get("SCIENCEC", os.path.join(ROOT, "target", "debug", "sciencec"))


# --- SUMMARY.md -------------------------------------------------------------

def read_summary():
    """`# Part` lines start a group; `- [Title](slug.md)` lines are chapters."""
    parts, chapters = [], []
    for line in open(os.path.join(HERE, "SUMMARY.md"), encoding="utf-8"):
        line = line.rstrip()
        if line.startswith("# "):
            parts.append((line[2:], []))
        m = re.match(r"- \[(.+)\]\((.+)\.md\)", line)
        if m:
            ch = {"title": m.group(1), "slug": m.group(2), "part": parts[-1][0]}
            parts[-1][1].append(ch)
            chapters.append(ch)
    return parts, chapters


# --- Markdown, the subset the book uses ---------------------------------------

def inline(text):
    out, i = [], 0
    for m in re.finditer(r"`([^`]+)`", text):
        out.append(inline_plain(text[i:m.start()]))
        out.append("<code>" + html.escape(m.group(1)) + "</code>")
        i = m.end()
    out.append(inline_plain(text[i:]))
    return "".join(out)


def inline_plain(text):
    text = html.escape(text, quote=False)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", text)
    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", lambda m: '<a href="%s">%s</a>'
                  % (m.group(2).replace(".md", ".html"), m.group(1)), text)
    return text


def slugify(text):
    return re.sub(r"[^a-z0-9]+", "-", re.sub(r"<[^>]+>|`", "", text).lower()).strip("-")


def render(md, chapter, examples):
    lines = md.split("\n")
    out, toc, i = [], [], 0
    para = []

    def flush():
        if para:
            out.append("<p>" + inline(" ".join(para)) + "</p>")
            para.clear()

    while i < len(lines):
        line = lines[i]
        fence = re.match(r"```(\w*)\s*(\w*)", line)
        if fence:
            flush()
            lang, mode = fence.group(1), fence.group(2)
            body = []
            i += 1
            while not lines[i].startswith("```"):
                body.append(lines[i])
                i += 1
            code = "\n".join(body)
            esc = html.escape(code)
            if lang == "science":
                examples.append({"code": code, "mode": mode or "check",
                                 "chapter": chapter["slug"], "line": i})
                cls = ' class="refused"' if mode == "fails" else ""
                out.append("<pre data-science%s><code>%s</code></pre>" % (cls, esc))
            elif lang == "output":
                if examples and examples[-1]["mode"] == "run":
                    examples[-1]["output"] = code
                out.append('<pre class="output"><code>%s</code></pre>' % esc)
            elif lang == "shell":
                out.append('<pre class="shell"><code>%s</code></pre>' % esc)
            elif lang == "error":
                out.append('<pre class="err"><code>%s</code></pre>' % esc)
            else:
                out.append("<pre><code>%s</code></pre>" % esc)
            i += 1
            continue
        h = re.match(r"(#{2,3}) (.+)", line)
        if h:
            flush()
            level, text = len(h.group(1)), h.group(2)
            anchor = slugify(text)
            if level == 2:
                toc.append((anchor, inline(text)))
            out.append('<h%d id="%s">%s<a class="anchor" href="#%s" aria-hidden="true">#</a></h%d>'
                       % (level, anchor, inline(text), anchor, level))
        elif line.startswith("> "):
            flush()
            quote = []
            while i < len(lines) and lines[i].startswith(">"):
                quote.append(lines[i][1:].strip())
                i += 1
            kind = "tip"
            if quote and re.match(r"\*\*(Note|Warning|Tip)", quote[0]):
                kind = re.match(r"\*\*(\w+)", quote[0]).group(1).lower()
            out.append('<aside class="callout-%s"><p>%s</p></aside>' % (kind, inline(" ".join(quote))))
            continue
        elif re.match(r"\s*([-*]|\d+\.) ", line):
            flush()
            ordered = bool(re.match(r"\s*\d+\.", line))
            items = []
            while i < len(lines) and (re.match(r"\s*([-*]|\d+\.) ", lines[i])
                                      or (lines[i].startswith("  ") and items)):
                m = re.match(r"\s*(?:[-*]|\d+\.) (.*)", lines[i])
                if m:
                    items.append(m.group(1))
                else:
                    items[-1] += " " + lines[i].strip()
                i += 1
            tag = "ol" if ordered else "ul"
            out.append("<%s>%s</%s>" % (tag, "".join("<li>%s</li>" % inline(x) for x in items), tag))
            continue
        elif line.startswith("|"):
            flush()
            rows = []
            while i < len(lines) and lines[i].startswith("|"):
                cells = [c.strip() for c in lines[i].strip().strip("|").split("|")]
                if not all(re.match(r"^:?-+:?$", c) for c in cells):
                    rows.append(cells)
                i += 1
            head = "".join("<th>%s</th>" % inline(c) for c in rows[0])
            body = "".join("<tr>%s</tr>" % "".join("<td>%s</td>" % inline(c) for c in r) for r in rows[1:])
            out.append('<div class="scroller"><table><thead><tr>%s</tr></thead><tbody>%s</tbody></table></div>'
                       % (head, body))
            continue
        elif line.strip() == "":
            flush()
        else:
            para.append(line.strip())
        i += 1
    flush()
    return "\n".join(out), toc


# --- The page -----------------------------------------------------------------

PAGE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>{title} &middot; The Science Book</title>
<meta name="description" content="{description}">
<link rel="icon" href="data:image/svg+xml,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 16 16%22><text y=%2213%22 font-size=%2213%22>&#8730;</text></svg>">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600&family=IBM+Plex+Sans:wght@400;500;600;700&family=IBM+Plex+Serif:ital,wght@0,400;0,500;1,400&display=swap">
<link rel="stylesheet" href="../assets/science.css">
<link rel="stylesheet" href="book.css">
</head>
<body class="book">

<header class="bar">
  <button class="menu" aria-label="Chapters" aria-expanded="false">&#9776;</button>
  <a class="brand" href="index.html"><span class="glyph">&#8730;</span> The Science Book</a>
  <nav><a href="../index.html">Home</a><a href="index.html" class="here">Learn</a><a href="../reference.html">Reference</a><a href="std.html">Library</a></nav>
</header>

<div class="layout">
<aside class="chapters">
{sidebar}
</aside>

<main>
<p class="crumb">{part}</p>
<h1>{title}</h1>
{body}
<nav class="pager">{prev}{next}</nav>
</main>

<aside class="onpage">
{onpage}
</aside>
</div>

<script src="../assets/highlight.js"></script>
<script src="book.js"></script>
</body>
</html>
"""


def sidebar(parts, current):
    out = []
    n = 0
    for name, chs in parts:
        out.append('<p class="group">%s</p><ol start="%d">' % (html.escape(name), n + 1 if chs else 0))
        for ch in chs:
            n += 1
            here = ' class="here" aria-current="page"' if ch["slug"] == current else ""
            out.append('<li><a href="%s.html"%s>%s</a></li>' % (ch["slug"], here, html.escape(ch["title"])))
        out.append("</ol>")
    return "\n".join(out)


def build():
    parts, chapters = read_summary()
    examples = []
    for idx, ch in enumerate(chapters):
        path = os.path.join(SRC, ch["slug"] + ".md")
        md = open(path, encoding="utf-8").read() if os.path.exists(path) else \
            "> **Note:** this chapter has not been written yet."
        # The first paragraph doubles as the meta description.
        first = next((p for p in md.split("\n\n") if p.strip() and not p.startswith(("#", "`", ">", "|", "-"))), "")
        body, toc = render(md, ch, examples)
        prev = chapters[idx - 1] if idx > 0 else None
        nxt = chapters[idx + 1] if idx + 1 < len(chapters) else None
        page = PAGE.format(
            title=html.escape(ch["title"]),
            description=html.escape(re.sub(r"[`*\[\]]|\(.*?\.md\)", "", " ".join(first.split()))[:300]),
            part=html.escape(ch["part"]),
            sidebar=sidebar(parts, ch["slug"]),
            body=body,
            onpage=("<p class=\"group\">On this page</p><ul>%s</ul>" % "".join(
                '<li><a href="#%s">%s</a></li>' % (a, t) for a, t in toc)) if toc else "",
            prev=('<a class="prev" href="%s.html"><span>Previous</span>%s</a>'
                  % (prev["slug"], html.escape(prev["title"]))) if prev else "<span></span>",
            next=('<a class="next" href="%s.html"><span>Next</span>%s</a>'
                  % (nxt["slug"], html.escape(nxt["title"]))) if nxt else "",
        )
        with open(os.path.join(HERE, ch["slug"] + ".html"), "w", encoding="utf-8") as f:
            f.write(page)
    print("built %d chapters, %d Science examples" % (len(chapters), len(examples)))
    return examples


# --- --check ------------------------------------------------------------------

def check(examples):
    failures = 0
    for ex in examples:
        if ex["mode"] == "norun":
            continue
        where = "%s.md (example ending near line %d)" % (ex["chapter"], ex["line"])
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "example.science")
            with open(path, "w", encoding="utf-8") as f:
                f.write(ex["code"] + "\n")
            verb = "run" if ex["mode"] == "run" else "check"
            r = subprocess.run([SCIENCEC, verb, path], cwd=d, capture_output=True, text=True, timeout=120)
            ok = r.returncode != 0 if ex["mode"] == "fails" else r.returncode == 0
            if ok and ex["mode"] == "run" and "output" in ex:
                ok = r.stdout.rstrip("\n") == ex["output"].rstrip("\n")
                if not ok:
                    print("FAIL %s: output differs\n--- expected\n%s\n--- got\n%s" % (where, ex["output"], r.stdout))
                    failures += 1
                    continue
            if not ok:
                print("FAIL %s [%s]\n%s%s" % (where, ex["mode"], r.stdout, r.stderr))
                failures += 1
    checked = sum(1 for e in examples if e["mode"] != "norun")
    print("%d of %d examples passed" % (checked - failures, checked))
    return failures


if __name__ == "__main__":
    examples = build()
    if "--check" in sys.argv:
        sys.exit(1 if check(examples) else 0)
