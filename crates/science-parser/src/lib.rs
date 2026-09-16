//! Science's parser: tokens in, syntax tree out.
//!
//! - [`ast`] is the tree itself. Every node carries its `Span`, per §9 of the
//!   design spec.
//! - [`dump`] renders a tree as readable indented text, which is what the
//!   snapshot tests compare and what the later phases print when they want to
//!   see what they were handed.
//! - [`parser`] is the recursive-descent parser, and [`parser::codes`] lists
//!   the syntax diagnostics, SC0100-SC0199.
//!
//! The parser never stops at the first error: it reports, synchronises on the
//! end of the line or the end of the block, and keeps going, so one pass can
//! report several problems.
//!
//! ```no_run
//! use science_diagnostics::FileId;
//! use science_parser::{parse_module, Dump};
//!
//! # let tokens: Vec<science_lexer::Token> = Vec::new();
//! let (module, diagnostics) = parse_module(&tokens, FileId(0));
//! print!("{}", module.dump());
//! assert!(!diagnostics.has_errors());
//! ```

pub mod ast;
pub mod dump;
pub mod parser;

pub use dump::{Dump, DumpWriter};
pub use parser::{codes, parse_module, Parser};
