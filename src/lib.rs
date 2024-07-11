extern crate dprint_core;

pub mod configuration;
pub mod handler;

#[cfg(feature = "process")]
pub use main::*;

pub use handler::format_bytes;
