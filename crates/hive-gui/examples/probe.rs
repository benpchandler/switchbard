fn main() {
    let listeners = hive_core::scan_listeners().unwrap();
    let repos = vec![
        hive_core::Repo { name: "alpha".into(), path: std::path::PathBuf::from("/Users/me/code/alpha") },
        hive_core::Repo { name: "delta".into(), path: std::path::PathBuf::from("/Users/me/code/delta") },
        hive_core::Repo { name: "gamma".into(), path: std::path::PathBuf::from("/Users/me/code/gamma") },
        hive_core::Repo { name: "beta".into(), path: std::path::PathBuf::from("/Users/me/code/beta") },
        hive_core::Repo { name: "hive".into(), path: std::path::PathBuf::from("/Users/me/code/hive") },
    ];
    let attributed = hive_core::attribute(&listeners, &repos);
    let total = attributed.len();
    let matched: Vec<_> = attributed.iter().filter(|l| l.repo_name.is_some()).collect();
    println!("Total listeners: {total}");
    println!("Attributed: {}", matched.len());
    for a in &matched {
        let cwd = a.listener.cwd.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        println!("  port={:<6} pid={:<6} cmd={:<20} repo={} cwd={}", a.listener.port, a.listener.pid, a.listener.command_name, a.repo_name.as_deref().unwrap_or(""), cwd);
    }
    println!("---unattributed sample (first 8)---");
    for a in attributed.iter().filter(|l| l.repo_name.is_none()).take(8) {
        let cwd = a.listener.cwd.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        println!("  port={:<6} pid={:<6} cmd={:<20} cwd={}", a.listener.port, a.listener.pid, a.listener.command_name, cwd);
    }
}
