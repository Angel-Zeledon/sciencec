//! A textual rendering of one body's analysis, for tests.
//!
//! # 1. What it prints, and what it deliberately does not
//!
//! Region variables, their solved point sets, the constraints with their causes
//! and the summary. **Not the liveness table**, which is an intermediate the
//! constraints already reflect, and printing it would make every test that
//! touches a body sensitive to a dataflow detail it is not about.
//!
//! # 2. Points are printed as `bb0[2]`, and ranges are collapsed
//!
//! A region on a body of three hundred points is three hundred numbers, and a
//! test that holds one is a test nobody reads. Consecutive points within one
//! block print as `bb1[0..4]`. The collapsing is by *block*, never across one,
//! because two blocks adjacent in the numbering need not be adjacent in the
//! CFG and a range that spanned them would claim something false.
//!
//! # 3. Nothing here is a snapshot format
//!
//! `science-mir`'s `dump` says the same of itself. This exists so that a test
//! can assert *"this loan's region ends before that line"* in one string
//! instead of six accessor calls, and a test that asserted the whole rendering
//! of a large body would be a test of the renderer.

use std::fmt::Write;

use science_mir::mir::Body;
use science_resolve::hir::DefTable;

use crate::analysis::BodyAnalysis;
use crate::points::{Bits, PointIndex};
use crate::regions::{describe_path, RegionKind};

/// One body's regions, constraints and summary.
pub fn analysis(defs: &DefTable, body: &Body, analysis: &BodyAnalysis) -> String {
    let mut out = String::new();
    let name = &defs.get(body.def()).name;
    let _ = writeln!(out, "regions of `{name}`:");

    for var in analysis.table.vars() {
        let described = match analysis.table.kind(var) {
            RegionKind::Local { local, position } => {
                let at = describe_path(defs, &position.path);
                let mutable = if position.mutable { "mutable " } else { "" };
                format!("{local} {mutable}at {at}")
            }
            RegionKind::Loan(borrow) => {
                let data = body.borrow_data(*borrow);
                format!("loan {} of {}", borrow.index(), crate::check::name_of(defs, body, &data.place))
            }
        };
        let _ = writeln!(
            out,
            "  {var}  {described}  = {}",
            region(&analysis.index, analysis.solution.region(var))
        );
    }

    let _ = writeln!(out, "constraints:");
    for constraint in analysis.constraints.all() {
        let _ = writeln!(
            out,
            "  {}: {} @ {}  {}",
            constraint.sup,
            constraint.sub,
            constraint.point,
            constraint.cause.described(defs)
        );
    }

    let _ = writeln!(out, "summary:");
    if analysis.summary.returns.is_empty() {
        let _ = writeln!(out, "  the result borrows nothing");
    }
    for (position, from) in &analysis.summary.returns {
        let at = describe_path(defs, &position.path);
        if from.is_empty() {
            let _ = writeln!(out, "  the result at {at} is determined by nothing");
            continue;
        }
        let sources: Vec<String> = from
            .iter()
            .map(|source| format!("parameter {}{}", source.param, describe_source(defs, source)))
            .collect();
        let _ = writeln!(out, "  the result at {at} borrows from {}", sources.join(", "));
    }
    out
}

fn describe_source(defs: &DefTable, source: &crate::summary::ParamRegion) -> String {
    if source.path.is_empty() {
        String::new()
    } else {
        format!(" at {}", describe_path(defs, &source.path))
    }
}

/// §2.
pub fn region(index: &PointIndex, set: &Bits) -> String {
    let points: Vec<_> = set.iter().map(|at| index.point(at)).collect();
    if points.is_empty() {
        return "{}".to_string();
    }
    let mut runs: Vec<(science_mir::mir::BlockId, u32, u32)> = Vec::new();
    for point in points {
        match runs.last_mut() {
            Some((block, _, end)) if *block == point.block && *end + 1 == point.statement => {
                *end = point.statement;
            }
            _ => runs.push((point.block, point.statement, point.statement)),
        }
    }
    let rendered: Vec<String> = runs
        .into_iter()
        .map(|(block, start, end)| {
            if start == end {
                format!("{block}[{start}]")
            } else {
                format!("{block}[{start}..{end}]")
            }
        })
        .collect();
    format!("{{{}}}", rendered.join(", "))
}
