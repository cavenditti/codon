//! FM rendering pipeline scaffolding.
//!
//! Phase-17 introduces a custom-Element pipeline that bypasses GPUI's
//! `Div` overhead. The pipeline lands in pieces — this module is the
//! umbrella where each piece (trace harness, custom row, custom column,
//! caches) is wired in turn. Right now only the trace harness lives
//! here; the row/column Elements arrive in
//! `TASK:phase-17/fm-render-custom-row` and
//! `TASK:phase-17/fm-render-custom-column`.

pub(crate) mod trace;
pub(crate) mod shaped_line_cache;
