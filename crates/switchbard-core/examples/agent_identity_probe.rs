//! Read-only native identity diagnostic; never sends a signal.
fn main() -> anyhow::Result<()> {
    let pid: u32 = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: agent_identity_probe PID"))?
        .parse()?;
    println!("{:#?}", switchbard_core::probe_agent_identity(pid)?);
    Ok(())
}
