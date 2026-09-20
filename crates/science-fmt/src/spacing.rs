//! Where a space goes between two adjacent tokens.
//!
//! The rule is stated as its negation: **a space goes between every pair of
//! tokens except the pairs listed here.** Written the other way round — a table
//! of pairs that get a space — a missing entry would glue two tokens into one,
//! and `a be b` would silently become `abeb`. Written this way a missing entry
//! costs a space that should not be there, which is ugly and not wrong.
//!
//! Everything the table does glue is a bracket, a separator, a `.`, or a unary
//! `-`, and none of those can merge with a neighbour into a different token.
//! [`crate::format_source`] re-lexes and re-parses its own output and refuses
//! to emit anything whose token stream or syntax tree changed, so this
//! reasoning is checked rather than trusted. [`HYPHENATED`] exists because it
//! was checked and the reasoning had a hole in it: a space is not always
//! invisible to the parser.

use science_lexer::TokenKind as K;

/// Whether a token can be the last token of an expression.
///
/// This is how a unary `-` is told from a binary one — `-` is unary exactly
/// when nothing that could be its left operand precedes it — and how a `(` is
/// told from a grouping parenthesis: `f(` is a call, `be (` is not.
pub fn ends_expression(kind: &K) -> bool {
    matches!(
        kind,
        K::Ident(_)
            | K::Int { .. }
            | K::Float { .. }
            | K::Str(_)
            | K::Char(_)
            | K::True
            | K::False
            | K::Null
            | K::SelfValue
            | K::SelfType
            | K::RParen
            | K::RBracket
            | K::RBrace
            | K::Question
            | K::Underscore
            | K::Each
            // An `f"…"` ends where its closing quote does, so `f"{n}".len()`
            // is a method call on a literal exactly as `"a".len()` is.
            | K::FStrEnd
    )
}

/// Whether `prev` and `next` are written with no space between them.
///
/// `prev_unary` says whether `prev` is a unary `-` or a borrow-prefix `&`; the
/// caller computes it once per token, because it depends on the token *before*
/// `prev`.
pub fn glued(prev: &K, next: &K, prev_unary: bool) -> bool {
    // An `f"…"` is glued end to end, and this comes before everything else.
    //
    // **The decision.** No space is ever written between any two of the five
    // tokens an interpolating literal is made of, nor between `{` and the
    // expression inside it.
    //
    // **The reason.** Here the table's own safety argument does not hold. Its
    // opening sentence is that a missing entry *"costs a space that should not
    // be there, which is ugly and not wrong"* — true everywhere a space is
    // between two tokens of code, and false inside a string, where a space is a
    // character of the value. `f"n is {n}"` rendered with the default rule
    // would come back as `f" n is { n } "`, which lexes to the same tokens and
    // is a different program. So the f-string tokens are listed as glued rather
    // than left to the default, and this comment is why the usual "a missing
    // entry is only ugly" reasoning does not cover them.
    if matches!(next, K::FStrText(_) | K::InterpStart | K::InterpEnd | K::FStrEnd)
        || matches!(prev, K::FStrStart | K::FStrText(_) | K::InterpStart)
    {
        return true;
    }

    // Decided by the right-hand token first: a closer, a separator or a
    // postfix never has a space in front of it, whatever precedes it.
    match next {
        K::RParen | K::RBracket | K::RBrace => return true,
        K::Comma | K::Colon | K::Semi => return true,
        K::Question => return true,
        K::DotDot | K::DotDotEq => return true,
        // `1.foo` would re-lex as a float, so a `.` after a number keeps its
        // space. Nothing in the language puts one there, and the exception
        // costs one comparison.
        K::Dot => return !matches!(prev, K::Int { .. } | K::Float { .. }),
        // `f(`, `)(`, `Self(` — a call or a parameter list. `be (`, `of (`,
        // `-> (` are grouping, and keep their space. A bracket immediately
        // inside another bracket is glued whatever it is: `((a))`, not `( (a))`.
        //
        // `assert(` is glued for the same reason a call is, though `assert`
        // does not end an expression and does not belong in
        // [`ends_expression`]: it is spelled like a call
        // (`TokenKind::Assert`'s decision) without being one, so this is the
        // one place that shape is asked about on its own rather than folded
        // into "can this precede a call's parenthesis".
        K::LParen => {
            return ends_expression(prev)
                || matches!(prev, K::LParen | K::LBracket | K::LBrace | K::Assert)
        }
        // `xs[i]` — indexing, for the same reason.
        K::LBracket => {
            return matches!(
                prev,
                K::Ident(_)
                    | K::RParen
                    | K::RBracket
                    | K::SelfValue
                    | K::SelfType
                    | K::LParen
                    | K::LBracket
                    | K::LBrace
            )
        }
        _ => {}
    }

    match prev {
        K::LParen | K::LBracket | K::LBrace => true,
        K::Dot | K::DotDot | K::DotDotEq => true,
        // `-1`, but `- -1` rather than `--1`: two minus signs run together are
        // still two tokens, and writing them that way is a dare.
        K::Minus if prev_unary => !matches!(next, K::Minus),
        // `&T`, `&mut T`, `&doc.title` — the borrow sigil of §4.3 is always
        // glued to what it borrows, `mut` included, and the only question is
        // whether *this* `&` is that sigil or the infix `BitAnd` of `a & b`.
        // `prev_unary` is the same "did the token before this one end an
        // expression" test `-` uses to tell its own two readings apart
        // (`parser.rs`'s `parse_type_atom`/`parse_unary` decide it the same
        // way, by grammar position rather than by the character), so a plain
        // `&` gets no exception here the way `Minus` needs one for `- -1`:
        // two ampersands in a row are only ever an infix one followed by a
        // borrow (`a & &b`), never two borrows, so gluing them into `&&`
        // cannot happen by accident.
        K::Amp if prev_unary => true,
        _ => false,
    }
}

/// The words the grammar spells with a hyphen, as the tokens they lex to.
///
/// `pkg-config` is three tokens and one word. `Parser::expect_pkg_config` says
/// so outright — *"the three tokens are only accepted when they are written
/// with nothing between them; `pkg - config` is not the name of anything"* —
/// and it enforces it by comparing spans. That makes it the one place outside
/// the indentation where the gap between two tokens changes what a program
/// means, and therefore the one place where the spacing table is not free to
/// have an opinion. The formatter would otherwise turn every `extern` block
/// with a `via` clause into a syntax error, which is what it did before this
/// table existed.
const HYPHENATED: &[[&str; 2]] = &[["pkg", "config"]];

/// Whether the join between `tokens[i - 1]` and `tokens[i]` falls inside a
/// hyphenated word that was written with no gap in it.
///
/// The source's own adjacency is required, so that `pkg - config` — which does
/// not parse — is not silently repaired into something that does. A formatter
/// that fixed programs would be a formatter whose output is not its input.
pub fn hyphenated_join(tokens: &[science_lexer::Token], i: usize) -> bool {
    let word = |at: usize, text: &str| {
        tokens.get(at).is_some_and(|t| matches!(&t.kind, K::Ident(name) if name == text))
    };
    let joined = |at: usize| {
        at > 0
            && tokens.get(at - 1).is_some_and(|previous: &science_lexer::Token| {
                previous.span.end == tokens[at].span.start
            })
    };
    HYPHENATED.iter().any(|[left, right]| {
        // The `-` itself, or the word after it.
        let hyphen = if tokens.get(i).is_some_and(|t| t.kind == K::Minus) {
            i
        } else if i >= 1 && tokens.get(i - 1).is_some_and(|t| t.kind == K::Minus) {
            i - 1
        } else {
            return false;
        };
        hyphen >= 1
            && word(hyphen - 1, left)
            && word(hyphen + 1, right)
            && joined(hyphen)
            && joined(hyphen + 1)
    })
}

/// The spacing facts about one file that a pair of tokens cannot supply.
///
/// Both are computed in one pass over the whole stream, because both depend on
/// tokens further away than the neighbour: whether a `-` is unary depends on
/// what precedes it, and whether a `(` is a call depends on what statement it
/// is in.
pub struct Spacing {
    unary: Vec<bool>,
    /// `Some(true)` glues the join in front of this token, `Some(false)` forces
    /// a space, `None` leaves it to [`glued`].
    forced: Vec<Option<bool>>,
}

impl Spacing {
    pub fn of(tokens: &[science_lexer::Token]) -> Spacing {
        let mut unary = vec![false; tokens.len()];
        let mut forced = vec![None; tokens.len()];
        let mut previous: Option<&K> = None;
        // `use` needs to be recognised at the start of a logical line; nothing
        // else in the language begins one, but a `use` inside an expression is
        // not a thing either, so tracking the line start is the honest way to
        // ask the question.
        let mut at_line_start = true;
        let mut in_use = false;
        let mut depth = 0usize;

        for (i, token) in tokens.iter().enumerate() {
            match token.kind {
                K::Newline | K::Indent | K::Dedent | K::Eof => {
                    at_line_start = true;
                    in_use = false;
                    depth = 0;
                    continue;
                }
                K::Minus | K::Amp => unary[i] = !previous.is_some_and(ends_expression),
                K::Use if at_line_start => in_use = true,
                K::LParen | K::LBracket | K::LBrace => {
                    // `use text.parser (Token, lex)` is a selection, not a
                    // call, so it keeps the space that says so. The corpus
                    // writes it with one and §4.4 gives `use` no argument
                    // list to confuse it with.
                    if in_use && depth == 0 {
                        forced[i] = Some(false);
                        in_use = false;
                    }
                    depth += 1;
                }
                K::RParen | K::RBracket | K::RBrace => depth = depth.saturating_sub(1),
                _ => {}
            }
            if hyphenated_join(tokens, i) {
                forced[i] = Some(true);
            }
            at_line_start = false;
            previous = Some(&token.kind);
        }
        Spacing { unary, forced }
    }

    /// Whether `tokens[i - 1]` and `tokens[i]` are written with no space.
    pub fn glued_at(&self, tokens: &[science_lexer::Token], i: usize) -> bool {
        match self.forced[i] {
            Some(decided) => decided,
            None => glued(&tokens[i - 1].kind, &tokens[i].kind, self.unary[i - 1]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_separator_never_has_a_space_in_front_of_it() {
        assert!(glued(&K::Ident("a".into()), &K::Comma, false));
        assert!(glued(&K::Ident("a".into()), &K::Colon, false));
        assert!(glued(&K::RParen, &K::Question, false));
    }

    #[test]
    fn a_call_paren_is_glued_and_a_grouping_paren_is_not() {
        assert!(glued(&K::Ident("f".into()), &K::LParen, false));
        assert!(glued(&K::RParen, &K::LParen, false));
        assert!(!glued(&K::Be, &K::LParen, false));
        assert!(!glued(&K::Of, &K::LParen, false));
        assert!(!glued(&K::Arrow, &K::LParen, false));
    }

    /// `assert` is spelled like a call and formatted like one, though it is a
    /// statement and not in [`ends_expression`].
    #[test]
    fn asserts_paren_is_glued_the_way_a_calls_is() {
        assert!(glued(&K::Assert, &K::LParen, false));
    }

    /// The bracket's two meanings, as spacing sees them.
    ///
    /// `indexing-and-array-literals.md` §6.2 decides the reading by parser
    /// position, and this table has always agreed with it by accident: a `[`
    /// after something that ends an expression is an index and is glued, and a
    /// `[` after `be`, `return` or a comma opens a literal and keeps its
    /// space. The rule needed no change when literals were added, which is
    /// what this test is here to say.
    #[test]
    fn a_prefix_bracket_opens_a_literal_and_a_postfix_one_indexes() {
        assert!(glued(&K::Ident("xs".into()), &K::LBracket, false));
        assert!(glued(&K::RParen, &K::LBracket, false));
        assert!(glued(&K::RBracket, &K::LBracket, false));
        assert!(!glued(&K::Be, &K::LBracket, false));
        assert!(!glued(&K::Return, &K::LBracket, false));
        assert!(!glued(&K::Comma, &K::LBracket, false));
    }

    #[test]
    fn a_binary_minus_keeps_its_spaces_and_a_unary_one_does_not() {
        assert!(!glued(&K::Minus, &K::Int { value: 1, base: science_lexer::IntBase::Dec, suffix: None }, false));
        assert!(glued(&K::Minus, &K::Int { value: 1, base: science_lexer::IntBase::Dec, suffix: None }, true));
        assert!(!glued(&K::Minus, &K::Minus, true));
    }

    #[test]
    fn a_hyphenated_word_keeps_its_hyphen() {
        use science_diagnostics::FileId;
        let source = "unsafe extern \"C\" library \"m\" via pkg-config \"m\":
    type T is I32
";
        let (tokens, _) = science_lexer::lex(FileId(0), source);
        let spacing = Spacing::of(&tokens);
        let hyphen = tokens.iter().position(|t| t.kind == K::Minus).expect("a hyphen");
        assert!(spacing.glued_at(&tokens, hyphen));
        assert!(spacing.glued_at(&tokens, hyphen + 1));

        // Written apart it stays apart: `pkg - config` does not parse, and a
        // formatter that repaired it would be changing the program.
        let apart = source.replace("pkg-config", "pkg - config");
        let (tokens, _) = science_lexer::lex(FileId(0), &apart);
        let spacing = Spacing::of(&tokens);
        let hyphen = tokens.iter().position(|t| t.kind == K::Minus).expect("a hyphen");
        assert!(!spacing.glued_at(&tokens, hyphen));
    }

    #[test]
    fn a_use_selection_is_not_a_call() {
        use science_diagnostics::FileId;
        let (tokens, _) = science_lexer::lex(FileId(0), "use text.parser (Token, lex)
");
        let spacing = Spacing::of(&tokens);
        let paren = tokens.iter().position(|t| t.kind == K::LParen).expect("a paren");
        assert!(!spacing.glued_at(&tokens, paren));
    }

    #[test]
    fn a_dot_after_a_number_is_not_glued() {
        assert!(!glued(&K::Int { value: 1, base: science_lexer::IntBase::Dec, suffix: None }, &K::Dot, false));
        assert!(glued(&K::Ident("a".into()), &K::Dot, false));
    }

    #[test]
    fn two_words_always_get_a_space() {
        assert!(!glued(&K::Let, &K::Ident("a".into()), false));
        assert!(!glued(&K::Ident("a".into()), &K::Be, false));
        assert!(!glued(&K::Is, &K::Not, false));
    }

    #[test]
    fn unary_is_decided_by_what_came_before() {
        use science_diagnostics::FileId;
        let (tokens, _) = science_lexer::lex(FileId(0), "let a be -1 - -2\n");
        let spacing = Spacing::of(&tokens);
        let minuses: Vec<bool> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == K::Minus)
            .map(|(i, _)| spacing.unary[i])
            .collect();
        assert_eq!(minuses, vec![true, false, true]);
    }
}
