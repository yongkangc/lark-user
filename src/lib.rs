#![forbid(unsafe_code)]

pub mod archive;
pub mod context;
pub mod error;
pub mod model;
mod private_files;
pub mod protocol;
pub mod session;
pub mod storage;

pub use error::{Error, ErrorCode, Result};
