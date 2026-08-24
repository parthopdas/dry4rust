//! `dry4rust` — a tree-edit-distance duplicate-code detector for Rust codebases.
//!
//! Dependencies flow **adapters → core**, never the reverse:
//!
//! - **core** (pure, std-only, side-effect-free): `tree`, `ted`, `similarity`,
//!   `dedup`, `detect`, `model`, `error`.
//! - **adapters** (each confines one third-party crate): `parse` (`syn`),
//!   `discovery` (`ignore`), `report` (`serde`).
//!
//! Only `error` and the discovery entry point are public API today; the
//! remaining modules are internal scaffolding filled in by later tasks.

pub mod error;

pub(crate) mod dedup;
pub(crate) mod detect;
pub(crate) mod model;
pub(crate) mod similarity;
pub(crate) mod ted;
pub(crate) mod tree;

pub(crate) mod discovery;
pub(crate) mod parse;
pub(crate) mod report;

pub use discovery::discover_rust_files;
