//! Core common items that are required by _all_ other crates in the `daedalus` crate graph.
#![warn(missing_docs)]

mod progress;
mod str;
mod switch;

#[doc(inline)]
pub use crate::str::Str;
#[doc(inline)]
pub use crate::switch::Switch;
#[doc(inline)]
pub use progress::*;
