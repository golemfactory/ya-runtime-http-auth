#![deny(missing_docs)]
//! Proxy management api client bindings.
//!
//! ### Example
//! ```
//! use ya_http_proxy_client::{ManagementApi, Result};
//!
//! async fn print_services() -> Result<()> {
//!     let api = ManagementApi::try_default()?;
//!     eprintln!("services={:?}", api.get_services().await?);
//!     Ok(())
//! }
//! ```
//!
//!
mod api;
mod error;
mod web;

/// Management API communication objects.
pub mod model;

pub use api::ManagementApi;
pub use error::Error;

/// A specialized Result type for proxy client operations.
pub type Result<T> = std::result::Result<T, Error>;

pub use web::{DEFAULT_MANAGEMENT_API_URL, ENV_MANAGEMENT_API_URL};
