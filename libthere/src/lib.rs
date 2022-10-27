#![allow(unused)]
#![forbid(unsafe_code)]

//! # libthere
//!
//! The shared code for that-goes-there. Encapsulates abstractions for things
//! like:
//!
//! - Command execution and log aggregation
//! - Logging and tracing
//! - Command planning and validations for targets
//!   - Compile various pieces of functionality down to sh scripts as much as
//!     possible, such as ensuring files/directories do/not exist.

pub mod executor;
pub mod log;
pub mod plan;
