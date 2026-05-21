//! Service-detection probe. Pass one or more repo paths and see what
//! services Switchbard would detect from their Procfile / package.json /
//! Makefile / docker-compose / scripts.
//!
//! Usage:
//!   cargo run --example probe_services -- /path/to/repo [/path/to/repo ...]

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: cargo run --example probe_services -- /path/to/repo [/path/to/repo ...]");
        std::process::exit(1);
    }
    for p in args {
        let path = PathBuf::from(&p);
        let svcs = switchbard_core::detect_services(&path);
        println!("{p}:");
        for s in svcs {
            println!("  {} → {}", s.name, s.command);
        }
    }
}
