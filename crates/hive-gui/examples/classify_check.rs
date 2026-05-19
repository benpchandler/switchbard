use hive_core::{detect_services, ServerLikelihood};
use std::path::PathBuf;

fn icon(l: ServerLikelihood) -> &'static str {
    match l {
        ServerLikelihood::Server => "✓ SERVER",
        ServerLikelihood::Maybe => "? maybe ",
        ServerLikelihood::NotServer => "✕ not-srv",
    }
}

fn main() {
    for p in ["/Users/me/code/alpha", "/Users/me/code/delta", "/Users/me/code/beta"] {
        println!("\n=== {p} ===");
        let svcs = detect_services(&PathBuf::from(p));
        for s in &svcs {
            println!("  [{}] {:32}  src={:?}", icon(s.likelihood), s.name, s.source);
        }
    }
}
