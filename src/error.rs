//! Error types for the `dry4rust` library (core; `thiserror`-based).
//!
//! The library returns typed errors; the binary bubbles these via `anyhow` and
//! maps them to a non-zero exit only for usage/internal failures. Adapter crates
//! (e.g. `syn`) stay confined to their modules, so parse failures are carried
//! here as a plain message rather than a foreign error type.

use thiserror::Error;

/// Errors returned by the `dry4rust` library.
#[derive(Debug, Error)]
pub enum Error {
    /// An I/O failure while discovering or reading a source file.
    #[error("i/o error at {path}: {source}")]
    Io {
        /// Path (normalized to `/`) the failure relates to.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A source file could not be parsed as valid Rust.
    #[error("failed to parse {path}: {message}")]
    Parse {
        /// Path (normalized to `/`) that failed to parse.
        path: String,
        /// Human-readable description of the parse failure.
        message: String,
    },
}

/// Convenience alias for results returned by this library.
pub type Result<T> = std::result::Result<T, Error>;
