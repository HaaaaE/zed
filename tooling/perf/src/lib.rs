//! Some constants and datatypes used in the Markdown Editor perf profiler.
//! Should only be consumed by the crate providing the matching macros.
//!
//! For usage documentation, see the docs on this crate's binary.

mod implementation;
#[cfg(test)]
mod implementation_tests;
pub use implementation::*;
