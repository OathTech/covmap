//! covmap — coverings and connections between repositories.
//!
//! See covmap/theory/main.tex for the model. This binary implements
//! the porcelain + plumbing described in §5 of that note.

use std::env;
use std::io;
use std::process::ExitCode;

mod cache;
mod cli;
mod connection;
mod covering;
mod diff;
mod fingerprint;
mod html;
mod iterator;
mod message;
mod migrate;
mod peer;
mod repo;
mod resolve;
mod segment;
mod store;
mod walk;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    ExitCode::from(report_dispatch_result(cli::dispatch(args)))
}

fn report_dispatch_result(result: io::Result<u8>) -> u8 {
    match result {
        Ok(code) => code,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            eprintln!("covmap: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_pipe_is_a_clean_consumer_exit() {
        assert_eq!(
            report_dispatch_result(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "consumer closed"
            ))),
            0
        );
    }
}
