//! Laying one logical line out over one or more physical lines.
//!
//! A logical line is the unit because it is the unit the lexer recognises: the
//! physical lines a logical line is spread over carry no meaning at all, so
//! joining them, splitting them differently, or leaving them alone are all the
//! same program. What is *not* free is where a split may go — only inside an
//! unclosed bracket, before a `.` that starts a method call, or before `where`,
//! which is exactly what `Lexer::eat_line_continuation` recognises — and that
//! is the whole of the rule this module obeys.
//!
//! The layout never adds or removes a token. It cannot add a trailing comma to
//! a list it exploded, because `(a, b)` and `(a, b,)` are two different token
//! streams and only the parser knows what the second one means; a formatter
//! that took that decision would be a second parser. The cost is that a list
//! the formatter joins keeps whatever comma the author left in it. The benefit
//! is that the formatter's output has the input's token stream, which is what
//! lets this module decide a layout without ever looking at the tree.

use crate::scan::Comment;
use crate::spacing;
use science_lexer::{Token, TokenKind as K};

/// Four spaces, which is what the whole corpus uses. See [`crate`].
pub const INDENT: usize = 4;

/// How many `.name(` links make a chain a chain.
///
/// Two is a call on a call — `self.summarize().truncate(80)` — and the corpus
/// writes those on one line. Three is a pipeline, and the corpus writes those
/// down the page whether or not they fit.
const LONG_CHAIN: usize = 3;

/// A bracketed group and the comma-separated items in it.
struct Group {
    open: usize,
    close: usize,
    items: Vec<Item>,
}

impl Group {
    /// Whether the last item is followed by a comma — the "magic trailing
    /// comma" that keeps a list exploded however short it is.
    fn magic_comma(&self) -> bool {
        self.items.last().is_some_and(|i| i.comma.is_some())
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// One comma-separated item. `hi` excludes the comma, which the layout prints
/// itself so that it lands at the end of the item's last physical line.
struct Item {
    lo: usize,
    hi: usize,
    comma: Option<usize>,
}

/// Everything the layout needs about the file it is laying out.
pub struct Layout<'a> {
    pub source: &'a str,
    pub tokens: &'a [Token],
    pub comments: &'a [Comment],
    pub line_starts: &'a [usize],
    pub spacing: &'a spacing::Spacing,
    pub width: usize,
}

impl<'a> Layout<'a> {
    /// The source text of one token, exactly as it was written.
    ///
    /// A token is never re-printed from its parsed value. `0xDeadBeef`,
    /// `1_000.000_1` and `"it\'s escaped"` all carry information the token no
    /// longer has, and a formatter that re-printed them would be deciding on
    /// the author's behalf that hexadecimal has a canonical case.
    fn text(&self, i: usize) -> &'a str {
        let span = self.tokens[i].span;
        &self.source[span.start as usize..span.end as usize]
    }

    fn start_of(&self, i: usize) -> usize {
        self.tokens[i].span.start as usize
    }

    fn end_of(&self, i: usize) -> usize {
        self.tokens[i].span.end as usize
    }

    fn line_at(&self, pos: usize) -> usize {
        crate::scan::line_of(self.line_starts, pos)
    }

    /// The tokens `lo..hi` on one line, with the spacing table applied.
    pub fn flat(&self, lo: usize, hi: usize) -> String {
        let mut out = String::new();
        for i in lo..hi {
            if i > lo && !self.spacing.glued_at(self.tokens, i) {
                out.push(' ');
            }
            out.push_str(self.text(i));
        }
        out
    }

    fn width_of(text: &str) -> usize {
        text.chars().count()
    }

    /// The comments whose hash falls in `[from, to)`.
    fn comments_in(&self, from: usize, to: usize) -> &'a [Comment] {
        let lo = self.comments.partition_point(|c| c.start < from);
        let hi = self.comments.partition_point(|c| c.start < to);
        &self.comments[lo..hi.max(lo)]
    }

    /// The comments strictly inside the token range — the ones the layout has
    /// to find a home for. A comment after the last token belongs to the
    /// caller, which prints it as the line's trailing comment.
    fn interior(&self, lo: usize, hi: usize) -> &'a [Comment] {
        self.comments_in(self.start_of(lo), self.end_of(hi - 1))
    }

    /// Whether any group anywhere inside `lo..hi` ends in a comma.
    fn has_magic_comma(&self, lo: usize, hi: usize) -> bool {
        (lo + 1..hi).any(|i| {
            matches!(self.tokens[i].kind, K::RParen | K::RBracket | K::RBrace)
                && self.tokens[i - 1].kind == K::Comma
        })
    }

    /// The group opened at `open`, split at its top-level commas.
    fn group_at(&self, open: usize, limit: usize) -> Option<Group> {
        let mut items = Vec::new();
        let mut depth = 0usize;
        let mut item_lo = open + 1;
        let mut i = open + 1;
        while i < limit {
            match self.tokens[i].kind {
                K::LParen | K::LBracket | K::LBrace => depth += 1,
                K::RParen | K::RBracket | K::RBrace => {
                    if depth == 0 {
                        if item_lo < i {
                            items.push(Item { lo: item_lo, hi: i, comma: None });
                        }
                        return Some(Group { open, close: i, items });
                    }
                    depth -= 1;
                }
                K::Comma if depth == 0 => {
                    if item_lo < i {
                        items.push(Item { lo: item_lo, hi: i, comma: Some(i) });
                    }
                    item_lo = i + 1;
                }
                _ => {}
            }
            i += 1;
        }
        // Unbalanced brackets mean the file did not lex cleanly, and the
        // caller has already refused it. Returning `None` keeps this total.
        None
    }

    /// The bracket groups at the top level of `lo..hi`.
    fn top_groups(&self, lo: usize, hi: usize) -> Vec<Group> {
        let mut out = Vec::new();
        let mut i = lo;
        while i < hi {
            if matches!(self.tokens[i].kind, K::LParen | K::LBracket | K::LBrace) {
                match self.group_at(i, hi) {
                    Some(group) => {
                        i = group.close + 1;
                        out.push(group);
                    }
                    None => return out,
                }
            } else {
                i += 1;
            }
        }
        out
    }

    /// The `.` tokens at the top level of `lo..hi` that begin a method call,
    /// when they form one chain that is the whole tail of the range.
    ///
    /// A link is `.` `name` `(`, and never the first token: `.name` alone is a
    /// field access, and breaking `doc.title` over two lines is legal but
    /// nobody wants it. The `(` is also what makes a broken chain read as a
    /// chain rather than as a list of dangling words.
    ///
    /// Two conditions beyond that, and both were found by running this over the
    /// corpus rather than by thinking about it.
    ///
    /// * **One chain, not several.** `if left.summarize().length() >=
    ///   right.summarize().length():` has four top-level links belonging to two
    ///   different receivers, and breaking before all four produces
    ///   `.length() >= right`, which parses and is nonsense.
    /// * **The chain ends the line.** A chain in the middle of an expression
    ///   leaves everything after it stranded on the last link's line. The
    ///   trailing `:` of a block header does not count as being after it,
    ///   because a block header is where most of these are written.
    fn chain_links(&self, lo: usize, hi: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut after: Option<usize> = None;
        let mut i = lo;
        while i < hi {
            if matches!(self.tokens[i].kind, K::LParen | K::LBracket | K::LBrace) {
                match self.group_at(i, hi) {
                    Some(group) => i = group.close + 1,
                    None => return Vec::new(),
                }
                continue;
            }
            let is_link = i > lo
                && self.tokens[i].kind == K::Dot
                && i + 2 < hi
                && matches!(self.tokens[i + 1].kind, K::Ident(_))
                && self.tokens[i + 2].kind == K::LParen;
            if !is_link {
                i += 1;
                continue;
            }
            // A link that does not begin where the previous one ended starts a
            // second chain, and this range has no single chain to break.
            if after.is_some_and(|end| end != i) {
                return Vec::new();
            }
            let Some(call) = self.group_at(i + 2, hi) else { return Vec::new() };
            out.push(i);
            after = Some(call.close + 1);
            i = call.close + 1;
        }

        let ends_the_line = after.is_some_and(|end| {
            end == hi || (end + 1 == hi && self.tokens[end].kind == K::Colon)
        });
        if ends_the_line {
            out
        } else {
            Vec::new()
        }
    }

    /// The first top-level `where`, which §4.4 allows a long signature to be
    /// broken before.
    fn where_at(&self, lo: usize, hi: usize) -> Option<usize> {
        let mut i = lo;
        while i < hi {
            if matches!(self.tokens[i].kind, K::LParen | K::LBracket | K::LBrace) {
                match self.group_at(i, hi) {
                    Some(group) => i = group.close + 1,
                    None => return None,
                }
            } else {
                if i > lo && self.tokens[i].kind == K::Where {
                    return Some(i);
                }
                i += 1;
            }
        }
        None
    }

    /// Lays `lo..hi` out at `indent`, as one or more physical lines.
    ///
    /// `None` means a comment inside the range has no position to go back to.
    /// The caller then reproduces the whole logical line as written: a comment
    /// moved somewhere its author did not choose is a smaller failure than a
    /// comment deleted, and both are larger than a line left alone.
    pub fn lay_out(&self, lo: usize, hi: usize, indent: usize) -> Option<Vec<String>> {
        debug_assert!(lo < hi);
        let comments = self.interior(lo, hi);
        let magic = self.has_magic_comma(lo, hi);
        let links = self.chain_links(lo, hi);
        let flat = self.flat(lo, hi);

        if comments.is_empty()
            && !magic
            && links.len() < LONG_CHAIN
            && indent + Self::width_of(&flat) <= self.width
        {
            return Some(vec![pad(indent) + &flat]);
        }

        // 1. A chain of three or more calls is broken whatever its width. It is
        //    the house style — `AGENTS.md` §4 writes one out, and three of the
        //    corpus files break one by hand — and it is the shape the language
        //    was given `eat_line_continuation` for.
        if links.len() >= LONG_CHAIN {
            if let Some(lines) = self.lay_out_split(lo, hi, &links, indent, INDENT) {
                return Some(lines);
            }
        }

        let groups = self.top_groups(lo, hi);

        // 2. A magic trailing comma is an instruction, not a measurement, so
        //    it is honoured before anything that depends on the width.
        if let Some(group) = groups.iter().filter(|g| !g.is_empty()).find(|g| g.magic_comma()) {
            if let Some(lines) = self.lay_out_group(lo, hi, group, indent) {
                return Some(lines);
            }
        }

        // 3. Two links break only when the line is too long for one.
        if links.len() >= 2 {
            if let Some(lines) = self.lay_out_split(lo, hi, &links, indent, INDENT) {
                return Some(lines);
            }
        }

        // 4. A signature breaks before `where`, which keeps the parameters on
        //    one line and moves the bounds below them rather than the reverse.
        //    The clause is indented two levels, not one, because one level is
        //    where the *body* goes and a reader cannot be asked to tell a
        //    bound from the first statement of the function.
        //    `examples/07_generics.science` indents its hand-broken signature
        //    continuations by eight spaces for the same reason.
        if let Some(at) = self.where_at(lo, hi) {
            if let Some(lines) = self.lay_out_split(lo, hi, &[at], indent, 2 * INDENT) {
                return Some(lines);
            }
        }

        // 5. Otherwise explode the widest bracket group. The widest is chosen
        //    rather than the first so that the break lands where the line is
        //    actually long, and it is a property of the tokens alone, which is
        //    what makes formatting twice give the same answer as formatting
        //    once.
        if let Some(group) = groups
            .iter()
            .filter(|g| !g.is_empty())
            .max_by_key(|g| Self::width_of(&self.flat(g.open, g.close + 1)))
        {
            if let Some(lines) = self.lay_out_group(lo, hi, group, indent) {
                return Some(lines);
            }
        }

        // 6. Nothing here may be split. An over-long line is the honest
        //    outcome: the alternative is a break the lexer would read as the
        //    start of a new statement.
        if comments.is_empty() {
            return Some(vec![pad(indent) + &flat]);
        }
        None
    }

    /// Breaks `lo..hi` before each offset in `at`, continuing at
    /// `indent + hang`.
    ///
    /// This is both the chain layout and the `where` layout: every offset in
    /// `at` is a point `eat_line_continuation` recognises, and what follows one
    /// is laid out again in case it is still too long.
    fn lay_out_split(
        &self,
        lo: usize,
        hi: usize,
        at: &[usize],
        indent: usize,
        hang: usize,
    ) -> Option<Vec<String>> {
        let mut bounds = Vec::with_capacity(at.len() + 2);
        bounds.push(lo);
        bounds.extend_from_slice(at);
        bounds.push(hi);

        let mut out = Vec::new();
        for segment in 0..bounds.len() - 1 {
            let (a, b) = (bounds[segment], bounds[segment + 1]);
            let inner = if segment == 0 { indent } else { indent + hang };
            if segment > 0 {
                self.place_comments(self.end_of(a - 1), self.start_of(a), inner, &mut out);
            }
            out.extend(self.lay_out(a, b, inner)?);
        }
        Some(out)
    }

    /// Explodes one bracket group: everything up to and including the opening
    /// bracket on the first line, one item per line, the closing bracket and
    /// whatever follows it on the last.
    fn lay_out_group(
        &self,
        lo: usize,
        hi: usize,
        group: &Group,
        indent: usize,
    ) -> Option<Vec<String>> {
        // A comment in the head or the tail has no line of its own to go on,
        // and moving it into the list would put it against a different item.
        if !self.comments_in(self.start_of(lo), self.end_of(group.open)).is_empty() {
            return None;
        }
        if !self.comments_in(self.start_of(group.close), self.end_of(hi - 1)).is_empty() {
            return None;
        }

        let inner = indent + INDENT;
        let mut out = vec![pad(indent) + &self.flat(lo, group.open + 1)];
        let mut anchor = self.end_of(group.open);

        for item in &group.items {
            self.place_comments(anchor, self.start_of(item.lo), inner, &mut out);
            out.extend(self.lay_out(item.lo, item.hi, inner)?);
            if item.comma.is_some() {
                out.last_mut().expect("the head line is always there").push(',');
            }
            anchor = match item.comma {
                Some(comma) => self.end_of(comma),
                None => self.end_of(item.hi - 1),
            };
        }

        self.place_comments(anchor, self.start_of(group.close), inner, &mut out);
        out.push(pad(indent) + &self.flat(group.close, hi));
        Some(out)
    }

    /// Puts the comments written between two tokens back.
    ///
    /// A comment written at the end of a line stays at the end of a line; one
    /// that had a line to itself keeps one. That relationship is the only
    /// thing about a comment's position that a reader was relying on, and the
    /// only thing reformatting can promise to preserve.
    fn place_comments(&self, from: usize, to: usize, indent: usize, out: &mut Vec<String>) {
        let anchor_line = self.line_at(from.saturating_sub(1));
        for comment in self.comments_in(from, to) {
            let text = &self.source[comment.start..comment.end];
            match out.last_mut() {
                Some(last) if self.line_at(comment.start) == anchor_line => {
                    last.push_str("  ");
                    last.push_str(text);
                }
                _ => out.push(pad(indent) + text),
            }
        }
    }

    /// The comment written after the last token of a logical line, on the same
    /// physical line as it.
    pub fn trailing_comment(&self, hi: usize) -> Option<&'a Comment> {
        let end = self.end_of(hi - 1);
        let comment = self.comments_in(end, self.source.len()).first()?;
        if self.line_at(comment.start) == self.line_at(end.saturating_sub(1)) {
            Some(comment)
        } else {
            None
        }
    }
}

fn pad(n: usize) -> String {
    " ".repeat(n)
}
