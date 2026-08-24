//! `dry4rust` binary entry point: `anyhow` wiring and exit codes.
//!
//! Parses the CLI (`cli`), runs the library façade (`dry4rust::run`), then
//! writes diagnostics to stderr and the report to stdout.
//!
//! Output discipline (N38): `render` already emits the exact trailing newline,
//! so the report goes out through a single `write_all` on a locked stdout —
//! never `println!`, which would double the newline and panic on `EPIPE`. A
//! `BrokenPipe` is quiet success, so `dry4rust | head` exits 0.
//!
//! Exit codes: 0 on any successful run (including "no duplicates" and runs that
//! skipped files); 1 on a whole-run failure; 2 for a clap usage error.

mod cli;

use std::io::{self, ErrorKind, Write};

use anyhow::Result;

fn main() -> Result<()> {
    let output = dry4rust::run(&cli::options())?;

    let mut stderr = io::stderr().lock();
    for diagnostic in &output.diagnostics {
        writeln!(stderr, "{diagnostic}")?;
    }

    let mut stdout = io::stdout().lock();
    if let Err(err) = stdout
        .write_all(output.report.as_bytes())
        .and_then(|()| stdout.flush())
    {
        if err.kind() != ErrorKind::BrokenPipe {
            return Err(err.into());
        }
    }
    Ok(())
}
