#![deny(missing_docs)]
//! Management API communication objects.

mod addr;
mod model;
#[doc(hidden)]
pub mod deser;

pub use addr::*;
pub use model::*;
