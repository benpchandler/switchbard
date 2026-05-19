use std::path::PathBuf;
fn main() {
    for p in [
        "/Users/me/code/alpha",
        "/Users/me/code/delta",
    ] {
        let path = PathBuf::from(p);
        let svcs = hive_core::detect_services(&path);
        println!("{p}:");
        for s in svcs {
            println!("  {} → {}", s.name, s.command);
        }
    }
}
