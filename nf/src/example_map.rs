#![allow(clippy::tabs_in_doc_comments)]

//! Example structs/types generated by the `#[maps]` procedural macro.
//!
//! The below code generates the types and traits in this module:
//!
//! ```rust
//! use crate::*;
//!
//! pub struct PodData {
//! 	pub a: u8,
//! 	pub b: bool,
//! 	pub c: u64,
//! }
//!
//! #[maps]
//! pub struct TestMaps {
//! 	plain: (u32, u64),
//! 	composite: (u32, PodData),
//! }
//! ```

use crate::*;

pub struct PodData {
	pub a: u8,
	pub b: bool,
	pub c: u64,
}

#[maps]
pub struct TestMaps {
	plain: (u32, u64),
	composite: (u32, PodData),
}
