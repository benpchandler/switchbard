//! Actual terminal process interruption, independent from the user's live app.

#[test]
fn terminal_signals_and_timer_checkpoint_restore_across_repo_spellings() {
    let output = std::process::Command::new("python3")
        .arg("-c")
        .arg(include_str!("harness/signal_resume.py"))
        .arg(env!("CARGO_BIN_EXE_sbt"))
        .output()
        .expect("python3 PTY driver");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
