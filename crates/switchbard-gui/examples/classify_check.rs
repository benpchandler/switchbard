//! Classifier check — pass repo paths and see how each detected entry
//! point is classified (server / ambiguous / not-server).
//!
//! Usage:
//!   cargo run --example classify_check -- /path/to/repo [/path/to/repo ...]

use std::path::PathBuf;
use switchbard_core::{detect_services, ServerLikelihood};

fn icon(l: ServerLikelihood) -> &'static str {
    match l {
        ServerLikelihood::Server => "✓ SERVER",
        ServerLikelihood::Maybe => "? maybe ",
        ServerLikelihood::NotServer => "✕ not-srv",
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: cargo run --example classify_check -- /path/to/repo [/path/to/repo ...]");
        std::process::exit(1);
    }
    for p in args {
        println!("\n=== {p} ===");
        let svcs = detect_services(&PathBuf::from(&p));
        for s in &svcs {
            println!(
                "  [{}] {:32}  src={:?}",
                icon(s.likelihood),
                s.name,
                s.source
            );
        }
    }
}
