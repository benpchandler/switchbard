use hive_core::{kill_pgid, scan_listeners, KillOutcome};
use std::collections::BTreeSet;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let target = std::env::args().nth(1).unwrap_or_else(|| "lyon-bundle".into());
    println!("target command substring: {target:?}");

    let listeners = scan_listeners()?;
    let matching: Vec<_> = listeners
        .iter()
        .filter(|l| l.command_name.contains(&target))
        .collect();
    let pgids: BTreeSet<i32> = matching.iter().map(|l| l.pgid).collect();
    println!(
        "{} matching listeners across {} unique PGIDs",
        matching.len(),
        pgids.len()
    );

    if pgids.is_empty() {
        println!("nothing to do.");
        return Ok(());
    }

    let mut terminated = 0usize;
    let mut killed = 0usize;
    let mut gone = 0usize;
    let mut errored = 0usize;
    for pgid in &pgids {
        let ports: Vec<u16> = matching
            .iter()
            .filter(|l| l.pgid == *pgid)
            .map(|l| l.port)
            .collect();
        let res = kill_pgid(*pgid, Duration::from_secs(3));
        let label = match res {
            Ok(KillOutcome::Terminated) => {
                terminated += 1;
                "terminated"
            }
            Ok(KillOutcome::Killed) => {
                killed += 1;
                "SIGKILLed"
            }
            Ok(KillOutcome::NotFound) => {
                gone += 1;
                "already gone"
            }
            Err(ref e) => {
                errored += 1;
                eprintln!("  pgid {pgid}: error {e}");
                "errored"
            }
        };
        println!(
            "  pgid {pgid} ({} ports: {:?}): {label}",
            ports.len(),
            ports
        );
    }

    println!(
        "summary: {terminated} terminated, {killed} SIGKILLed, {gone} already-gone, {errored} errored"
    );

    std::thread::sleep(Duration::from_millis(500));
    let after = scan_listeners()?;
    let remaining = after
        .iter()
        .filter(|l| l.command_name.contains(&target))
        .count();
    println!("post-sweep listener count matching '{target}': {remaining}");

    Ok(())
}
