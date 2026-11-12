//! Virtual file system over Arcanum's game data.
//!
//! [`ArcanumArchive`] reads one Troika `.dat` archive. [`Vfs`] layers an optional
//! override directory and any number of archives behind one case-insensitive,
//! separator-insensitive path namespace.

mod archive;
mod vfs;

pub use archive::{ArcanumArchive, TocEntry};
pub use vfs::Vfs;
