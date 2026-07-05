//! covmap — coverings and connections between repositories.
//!
//! See covmap/theory/main.tex for the model. This binary implements
//! the porcelain + plumbing described in §5 of that note.

use std::env;
use std::process::ExitCode;

mod cli;
mod connection;
mod covering;
mod diff;
mod fingerprint;
mod html;
mod repo;
mod segment;
mod store;
mod walk;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match cli::dispatch(args) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("covmap: {e}");
            ExitCode::from(1)
        }
    }
}
