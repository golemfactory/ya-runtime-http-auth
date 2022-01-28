pub mod web;

pub mod api;
pub mod deser;
pub mod error;
pub mod model;

pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;
