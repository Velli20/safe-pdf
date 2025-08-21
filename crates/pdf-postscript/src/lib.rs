//! A tiny PostScript-inspired stack calculator used inside the PDF processing pipeline.
//!
//! This crate provides:
//! * A small set of PostScript arithmetic, comparison, logical and stack
//!   manipulation operators (see [`operator::Operator`]).
//! * A parser (`parser::parse_tokens`) that turns token slices into an
//!   executable operator list supporting nested procedure blocks for `if` / `ifelse`.
//! * An interpreter (`calculator::execute` / `calculator::evaluate_postscript`) that
//!   evaluates the operators against a numeric operand stack.
//!
//! The implementation is deliberately minimal and only models what is required
//! by the surrounding PDF functionality; it is not a full PostScript engine.
//! Behavior differences (e.g. only numeric operands, booleans represented as 0.0 / 1.0)
//! are intentional simplifications.

pub mod calculator;
pub mod operator;
pub mod parser;
