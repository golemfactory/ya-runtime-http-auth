pub mod api;
pub mod error;
pub mod web;

pub use error::Error;
pub use web::WebClient;

pub type Result<T> = std::result::Result<T, Error>;
