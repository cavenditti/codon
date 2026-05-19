//! FM rendering pipeline scaffolding.
//!
//! Phase-17 introduces a custom-Element pipeline that bypasses GPUI's
//! `Div` overhead. The pipeline lands in pieces — this module is the
//! umbrella where each piece (trace harness, custom row, custom column,
//! caches) is wired in turn.

pub(crate) mod row;
pub(crate) mod shaped_line_cache;
pub(crate) mod trace;
