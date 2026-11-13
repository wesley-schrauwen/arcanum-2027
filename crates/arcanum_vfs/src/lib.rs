//! Virtual file system over Arcanum's game data.
//!
//! [`ArcanumArchive`] reads one Troika `.dat` archive. [`Vfs`] layers an optional
//! override directory and any number of archives behind one case-insensitive,
//! separator-insensitive path namespace.

mod archive;
mod vfs;

pub use archive::{ArcanumArchive, TocEntry};

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;
pub use vfs::Vfs;
