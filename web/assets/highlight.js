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
  var DECLARE = ("function type choice interface implements has of borrowed any use " +
    "public const extern unsafe let be mutable where giving").split(" ");

  // Control flow, and the operators spelled as words.
  var CONTROL = ("if else match for each in loop return break continue try and or not " +
    "is as self Self true false").split(" ");

  // §13's "reserved, not yet used". Coloured differently on purpose: a reader
  // who meets one in a sample should see that it is not an ordinary name.
  var RESERVED = ("agent tool prompt spawn send receive durable checkpoint resume supervise " +
    "async await tensor shape model equation mod pure parallel on with yield assert move " +
    "static macro union kernel import").split(" ");

  function set(words) {
    var m = Object.create(null);
    words.forEach(function (w) { m[w] = true; });
    return m;
  }
  var DE = set(DECLARE), CT = set(CONTROL), RS = set(RESERVED);

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

  function classOf(word, after) {
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

  function highlight(src) {
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
        var cls = classOf(m[5], after);
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
      blocks[i].innerHTML = "<code>" + highlight(blocks[i].textContent) + "</code>";
    }
    var banks = document.querySelectorAll("[data-words]");
    for (var j = 0; j < banks.length; j++) {
      var which = banks[j].getAttribute("data-words");
      var words = which === "keywords" ? DECLARE.concat(CONTROL)
                : which === "reserved" ? RESERVED
                : [];
      banks[j].innerHTML = words.map(function (w) {
        return "<li>" + esc(w) + "</li>";
      }).join("");
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", run);
  } else {
    run();
  }
})();
