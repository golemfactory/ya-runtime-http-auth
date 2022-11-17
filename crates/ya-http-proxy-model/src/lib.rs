#![deny(missing_docs)]
//! Management API communication objects.

mod addr;
#[doc(hidden)]
pub mod deser;
mod model;

pub use addr::*;
pub use model::*;
