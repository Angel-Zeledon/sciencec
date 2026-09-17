// Science — syntax highlighting for `<pre data-science>` blocks.
//
// Written here rather than pulled from a library because no highlighter knows
// this language, and teaching one costs more than the hundred lines below. The
// token classes match the VS Code themes in `editors/vscode/`, so a sample on
// the site and the same file in an editor are coloured the same way. Where the
// two disagreed, the grammar won: see `classOf` below.
//
// The word lists are the authority these pages share: when `from_word` in
// `crates/science-lexer/src/token.rs` changes, this file changes with it.
//
// No build step, no dependency, no module system. A `<script src>` per page.

(function () {
  "use strict";

  // Declarations and bindings — the words that introduce something.
  //
  // `def` replaced `function` in syntax revision 3, and the rename reached the
  // lexer with it: token.rs has `"def" => Function` and `function` is now an
  // ordinary identifier. Writing the old word where a declaration belongs is
  // SC0156, which carries the one-word fix.
  //
  // `tool` joined this list from RESERVED: it left `ReservedWord` and became a
  // declaration keyword of its own (`token.rs` has `"tool" => Tool`, no longer
  // `Reserved(Tool)`), because a `tool` declaration checks five things a `def`
  // does not — no generics, no borrowed parameter, no receiver, a required
  // `##` description, a body — and a marker that changes nothing would have
  // stayed a reservation.
  var DECLARE = ("def tool type choice interface implements has of borrowed any use " +
    "public const extern unsafe let be mutable where giving").split(" ");

  // Control flow, the operators spelled as words, and the literals.
  //
  // `try` left this list with syntax revision 2 §3, which removed `Result` and
  // `try` together; `null` joined it by §7, which notes the literal was missed
  // in the revision's first draft. Both moves have since reached the lexer:
  // token.rs has `"null" => Null` and no `try` at all, so this list and
  // `from_word` agree word for word. `try` is an ordinary identifier now, and
  // using it as the old prefix is SC0155 — a migration diagnostic with no fix,
  // because the new model has no one expression to swap in.
  var CONTROL = ("if else match for each in loop return break continue and or not " +
    "is as self Self true false null").split(" ");

  // §13's "reserved, not yet used". Coloured differently on purpose: a reader
  // who meets one in a sample should see that it is not an ordinary name.
  var RESERVED = ("agent prompt spawn send receive durable checkpoint resume supervise " +
    "async await tensor shape model equation mod pure parallel on with yield assert move " +
    "static macro union kernel import").split(" ");

  // §13's third list: free on purpose, because each is a common variable name
  // in the code of the people Science is for. Listed here only to fill the bank
  // on the reference page; it takes no part in highlighting.
  var NEVER = "grad dim dims axis device dtype unit alias".split(" ");

  // Spellings that have been REMOVED from the language. They are struck through
  // rather than coloured, and only inside a block marked `data-legacy`, which
  // is the one place the site still prints them — the before/after pairing in
  // the Errors section. Scoping it to that attribute is deliberate: `Result`
  // and `Some` are ordinary names a future sample may legitimately use, and a
  // global list would strike them wherever they appeared.
  var REMOVED = "try Result Option Ok Err Some None".split(" ");

  function set(words) {
    var m = Object.create(null);
    words.forEach(function (w) { m[w] = true; });
    return m;
  }
  var DE = set(DECLARE), CT = set(CONTROL), RS = set(RESERVED), RM = set(REMOVED);

  function esc(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  // Comment, string, char, number, word, operator, punctuation. The ordering
  // *is* the algorithm: a keyword inside a comment or a string must never win,
  // and putting the word case after the literals is what guarantees it.
  var TOKEN = new RegExp(
    "(#[^\\n]*)" +                                  // 1 comment
    "|(\"(?:[^\"\\\\]|\\\\[\\s\\S])*\")" +          // 2 string
    "|('(?:[^'\\\\]|\\\\[\\s\\S])*')" +             // 3 char
    "|(\\b\\d[0-9A-Za-z_.]*)" +                     // 4 number
    "|([A-Za-z_][A-Za-z0-9_]*)" +                   // 5 word
    "|(->|\\*\\*|\\.\\.=|\\.\\.|>=|<=|<<|>>|[+\\-*/%@<>&|^?])" + // 6 operator
    "|([(){}\\[\\],:;.])",                          // 7 punctuation
    "g");

  function classOf(word, after, legacy) {
    if (legacy && RM[word]) { return "rm"; }
    if (DE[word]) { return "k"; }
    if (CT[word]) { return "kc"; }
    if (RS[word]) { return "r"; }
    // Capitalisation is tested BEFORE the `:` rule, and the order matters:
    // `type Doc:` and `choice Format:` both put a type name immediately before
    // a colon, and testing `:` first would colour the declared type as a label.
    // The TextMate grammar in editors/vscode/ makes the same choice; this file
    // was changed to agree with it, not the other way round.
    if (/^[A-Z]/.test(word)) { return "t"; }
    // A call is a name followed immediately by `(`. This is what separates
    // `truncate(80)` from a field access, with no scope knowledge at all.
    if (after === "(") { return "f"; }
    // A label or field: `title:` in a construction, a parameter, a match arm.
    if (after === ":") { return "pa"; }
    return "";
  }

  function highlight(src, legacy) {
    var out = "", last = 0, m;
    TOKEN.lastIndex = 0;
    while ((m = TOKEN.exec(src)) !== null) {
      out += esc(src.slice(last, m.index));
      last = TOKEN.lastIndex;

      if (m[1]) {
        out += '<span class="cm">' + esc(m[1]) + "</span>";
      } else if (m[2] || m[3]) {
        out += '<span class="s">' + esc(m[2] || m[3]) + "</span>";
      } else if (m[4]) {
        out += '<span class="n">' + esc(m[4]) + "</span>";
      } else if (m[5]) {
        // Look past any spaces to the next significant character, so
        // `map (each.x)` and `map(each.x)` are read the same way.
        var rest = src.slice(last);
        var after = (rest.match(/^[ \t]*(\S)/) || [])[1] || "";
        var cls = classOf(m[5], after, legacy);
        out += cls ? '<span class="' + cls + '">' + esc(m[5]) + "</span>" : esc(m[5]);
      } else if (m[6]) {
        out += '<span class="o">' + esc(m[6]) + "</span>";
      } else {
        out += '<span class="pu">' + esc(m[7]) + "</span>";
      }
    }
    return out + esc(src.slice(last));
  }

  function run() {
    var blocks = document.querySelectorAll("pre[data-science]");
    for (var i = 0; i < blocks.length; i++) {
      var legacy = blocks[i].hasAttribute("data-legacy");
      blocks[i].innerHTML = "<code>" + highlight(blocks[i].textContent, legacy) + "</code>";
    }
    var banks = document.querySelectorAll("[data-words]");
    for (var j = 0; j < banks.length; j++) {
      var which = banks[j].getAttribute("data-words");
      var words = which === "keywords" ? DECLARE.concat(CONTROL)
                : which === "reserved" ? RESERVED
                : which === "never" ? NEVER
                : [];
      banks[j].innerHTML = words.map(function (w) {
        return "<li>" + esc(w) + "</li>";
      }).join("");
    }
  }

  // The sidebar's "you are here" mark.
  //
  // An IntersectionObserver rather than a scroll handler: the browser does the
  // work off the main thread and there is no throttling to get wrong. The
  // margin pins the trigger line near the top of the viewport, so the section
  // highlighted is the one being read, not the one about to appear.
  //
  // If the API is missing the sidebar is still a list of working links, which
  // is the whole of its job; the highlight is the part that can be lost.
  function spy() {
    var links = document.querySelectorAll(".sidebar a[href^='#']");
    if (!links.length || !window.IntersectionObserver) { return; }

    var byId = Object.create(null);
    var targets = [];
    for (var i = 0; i < links.length; i++) {
      var id = links[i].getAttribute("href").slice(1);
      var section = document.getElementById(id);
      if (section) { byId[id] = links[i]; targets.push(section); }
    }

    var visible = Object.create(null);
    var observer = new IntersectionObserver(function (entries) {
      for (var i = 0; i < entries.length; i++) {
        visible[entries[i].target.id] = entries[i].isIntersecting;
      }
      // The first section still in the band wins, so scrolling up and down
      // over the same boundary does not flicker between two neighbours.
      var chosen = null;
      for (var j = 0; j < targets.length; j++) {
        if (visible[targets[j].id]) { chosen = targets[j].id; break; }
      }
      for (var k in byId) { byId[k].classList.toggle("here", k === chosen); }
    }, { rootMargin: "-10% 0px -75% 0px" });

    for (var t = 0; t < targets.length; t++) { observer.observe(targets[t]); }
  }

  // A copy button on every code block.
  //
  // `navigator.clipboard` needs a secure context, so it is absent over plain
  // http and on a `file://` page — which is exactly how `web/README.md` tells
  // a contributor to open these files. The button is therefore not added at
  // all when the API is missing, rather than added and left to fail: a control
  // that does nothing is worse than no control.
  //
  // The text is read before highlighting would have wrapped it in spans, and
  // `textContent` on the `pre` would include the button's own label, so the
  // source is captured from the `<code>` element instead.
  function copiers() {
    if (!navigator.clipboard || !navigator.clipboard.writeText) { return; }
    var blocks = document.querySelectorAll("pre");
    for (var i = 0; i < blocks.length; i++) {
      addCopy(blocks[i]);
    }
  }

  function addCopy(pre) {
    var code = pre.querySelector("code") || pre;
    var button = document.createElement("button");
    button.type = "button";
    button.className = "copy";
    button.textContent = "Copy";
    button.setAttribute("aria-label", "Copy this code to the clipboard");
    button.addEventListener("click", function () {
      navigator.clipboard.writeText(code.textContent).then(function () {
        settle(button, "Copied");
      }, function () {
        settle(button, "Failed");
      });
    });
    pre.appendChild(button);

    // Keep the button in the block's visible top-right corner.
    //
    // The button is `position: absolute` inside a `pre` that is
    // `overflow-x: auto`, and an absolutely positioned child of a scroll
    // container is laid out against the *scroll* origin, not the visible
    // one. So it scrolled away with the code. On a desktop that is a rare
    // annoyance; on a phone almost every listing on this site is wider than
    // the screen, so the first sideways swipe took the only copy control off
    // the left edge for good.
    //
    // The stylesheet reads `--scroll-x` and cancels it out with `translate`.
    // Writing a custom property rather than the transform directly keeps the
    // decision about *how* the button moves in the stylesheet with the rest
    // of the motion, and keeps this to reporting a number. Passive, because
    // it never calls preventDefault and a scroll handler that might is a
    // scroll handler the browser cannot fast-path.
    pre.addEventListener("scroll", function () {
      pre.style.setProperty("--scroll-x", pre.scrollLeft + "px");
    }, { passive: true });
  }

  function settle(button, word) {
    button.textContent = word;
    button.setAttribute("data-done", "");
    window.setTimeout(function () {
      button.textContent = "Copy";
      button.removeAttribute("data-done");
    }, 1400);
  }

  function start() { run(); spy(); copiers(); }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
