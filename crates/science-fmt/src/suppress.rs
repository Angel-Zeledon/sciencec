//! `# fmt: off`, `# fmt: on` and `# fmt: skip` — how an author says "this is
//! the point, leave it".
//!
//! The formatter was run over the whole corpus and came back with one verdict:
//! seven files changed, and four of them exist to *demonstrate* the very
//! properties the formatter normalises.
//! `examples/14_line_continuation.science` shows that "operators may sit at the
//! end or the start of a line" and that "the indentation inside is free-form";
//! `examples/15_comments.science` has two comments whose own text asserts the
//! column they are written at; `examples/16_indentation.science` shows that
//! "blank lines inside a block do not close it, however many there are"; and
//! `examples/20_extern.science` groups `m: BlasInt, n: BlasInt, k: BlasInt` on
//! one line because that triple is one thing in the C prototype. The formatter
//! is right about the language in all four cases, and destroys the
//! demonstration in all four.
//!
//! Before this module the only escape hatch was the magic trailing comma, and
//! it covers one of the four. It cannot cover the others: it exists for
//! comma-separated lists, and two of those cases are parenthesised
//! *expressions*, which have no commas and never will.
//!
//! # The markers
//!
//! **Decision.** Three of them, all comments, all on a line of their own:
//!
//! ```text
//! # fmt: off      everything from here to the matching `# fmt: on` is
//! # fmt: on       reproduced as written, both markers included
//! # fmt: skip     the next statement is reproduced as written
//! ```
//!
//! **Reason.** They are comments. The language already has comments, `scan.rs`
//! already carries every one of them as a first-class position, and a token,
//! an attribute or a pragma would all be new grammar for a thing the compiler
//! must then ignore. The argument that was used to decline an escape hatch for
//! chains — "adding one would mean inventing a token the language does not
//! have" — is the argument *for* this one, because a comment is not invented.
//!
//! Two forms and not one, because the two questions are different. A region
//! covers `20_extern.science`'s six hand-grouped signatures without six pairs
//! of markers; `# fmt: skip` covers a single construct without the second line
//! that says where the exception stops. The `skip` form is exactly the region
//! `# fmt: off` … `# fmt: on` would be if it were written around one statement,
//! and it is implemented as that.
//!
//! **Cost.** Two spellings for one idea, and a file can now contain source the
//! formatter will never touch — which is the point, and which also means the
//! guarantee "a formatted file is canonical" is now "a formatted file is
//! canonical except where it says otherwise". `sciencec fmt` has no `--force`
//! and should not get one: a flag that overrode the markers would make them
//! advisory, and an advisory escape hatch is not one.
//!
//! # What a marker may be written on
//!
//! **Decision.** A marker must be a comment on a line of its own, standing
//! between statements. A `# fmt: skip` written at the end of a line of code is
//! an ordinary comment and does nothing, and so is any marker written inside a
//! bracketed list.
//!
//! **Reason.** Black spells it as a trailing comment, and that does not carry
//! over. A Science logical line can span a dozen physical lines, so a trailing
//! comment on one lands on the *last* of them — `)  # fmt: skip`, four lines
//! below the construct it governs. A marker above the thing it governs reads
//! the way every other annotation in every language does. Inside a bracket the
//! marker would be a comment the layout has to place, which is the problem it
//! exists to avoid.
//!
//! **Cost.** Someone who has used Black will write the trailing form, and it
//! will silently do nothing. There is no diagnostic for it, because "a comment
//! that looks like a directive" is not something a formatter can police
//! without policing prose.
//!
//! # An unmatched `# fmt: off`
//!
//! **Decision.** It suppresses formatting to the end of the file, and
//! [`crate::format_source`] reports a warning (`SC0901`) naming it.
//!
//! **Reason.** The two mistakes are not symmetric. Treating an unmatched `off`
//! as a no-op would reformat code the author asked to be left alone, which is
//! precisely the failure the mechanism exists to prevent. Running to the end of
//! the file leaves *more* alone than was meant, which is visible in the diff
//! and costs nothing but tidiness. Refusing the file outright would make a
//! typo in a comment stop the build. So: do the forgiving thing, and say so out
//! loud rather than silently.
//!
//! **Cost.** A warning on every run until it is fixed, on a command that is
//! otherwise silent when it has nothing to say.
//!
//! A `# fmt: on` with no `# fmt: off` above it is an ordinary comment. It is
//! not warned about: it changes nothing, and warning about it would mean
//! deciding that a sentence beginning "fmt: on" is always a directive.

use crate::scan::{line_of, Comment};

/// One of the three markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Off,
    On,
    Skip,
}

/// Reads a comment as a marker, if it is one.
///
/// Exactly one `#`, then `fmt:`, then the word, and nothing else on the line.
/// A `##` run is documentation and never a directive — the lexer carries it to
/// the declaration below as trivia, and a directive that was also a doc comment
/// would say two things at once.
pub fn marker(text: &str) -> Option<Marker> {
    let rest = text.strip_prefix('#')?;
    if rest.starts_with('#') {
        return None;
    }
    match rest.trim() {
        "fmt: off" => Some(Marker::Off),
        "fmt: on" => Some(Marker::On),
        "fmt: skip" => Some(Marker::Skip),
        _ => None,
    }
}

/// A run of physical lines reproduced as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// The first physical line, inclusive — the line the marker is on.
    pub first: usize,
    /// The last physical line, inclusive.
    pub last: usize,
}

impl Region {
    pub fn holds(&self, line: usize) -> bool {
        self.first <= line && line <= self.last
    }
}

/// Every suppressed region, and the byte offset of every `# fmt: off` that had
/// no `# fmt: on` after it.
pub struct Suppressed {
    pub regions: Vec<Region>,
    pub unmatched: Vec<usize>,
}

/// Works out what the markers in a file suppress.
///
/// `free` holds the indices of the comments that stand between statements —
/// the only ones that can be markers — and `statements` the physical line range
/// of each logical line, in source order. A marker inside a region is text: the
/// scan does not look at it, which is why a `# fmt: off` between two others is
/// not a second region.
pub fn suppressed(
    source: &str,
    comments: &[Comment],
    free: &[usize],
    line_starts: &[usize],
    statements: &[(usize, usize)],
) -> Suppressed {
    let mut regions = Vec::new();
    let mut unmatched = Vec::new();
    let last_line_of_file = line_starts.len().saturating_sub(1);

    let text = |c: &Comment| &source[c.start..c.end];
    let mut i = 0;
    while i < free.len() {
        let comment = &comments[free[i]];
        let first = line_of(line_starts, comment.start);
        match marker(text(comment)) {
            Some(Marker::Off) => {
                let close = free[i + 1..]
                    .iter()
                    .position(|&c| marker(text(&comments[c])) == Some(Marker::On))
                    .map(|offset| i + 1 + offset);
                match close {
                    Some(j) => {
                        regions.push(Region {
                            first,
                            last: line_of(line_starts, comments[free[j]].start),
                        });
                        i = j + 1;
                    }
                    None => {
                        unmatched.push(comment.start);
                        regions.push(Region { first, last: last_line_of_file });
                        break;
                    }
                }
            }
            Some(Marker::Skip) => {
                // The next statement, and everything written between the
                // marker and it: a comment between the two belongs to the
                // statement, and freezing half of a thing is not freezing it.
                let last = statements
                    .iter()
                    .find(|(begins, _)| *begins > first)
                    .map_or(first, |(_, ends)| *ends);
                regions.push(Region { first, last });
                while i < free.len() && line_of(line_starts, comments[free[i]].start) <= last {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    Suppressed { regions, unmatched }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_is_one_hash_and_nothing_else_on_the_line() {
        assert_eq!(marker("# fmt: off"), Some(Marker::Off));
        assert_eq!(marker("#fmt: on"), Some(Marker::On));
        assert_eq!(marker("#   fmt: skip   "), Some(Marker::Skip));
        assert_eq!(marker("## fmt: off"), None, "a doc run is not a directive");
        assert_eq!(marker("# fmt: off for now"), None);
        assert_eq!(marker("# fmt:off"), None, "one spelling, not several");
        assert_eq!(marker("# format: off"), None);
        assert_eq!(marker("# a note about fmt: off"), None);
    }
}
