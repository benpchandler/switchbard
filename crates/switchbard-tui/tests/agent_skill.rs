//! Exercise shipped instructions through the installed command interface.
use std::fs;
use std::os::unix::fs::symlink;
use std::process::{Command, Output};
use tempfile::TempDir;

fn run(home: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sbt"))
        .args(args)
        .env("HOME", home.path())
        .output()
        .expect("execute sbt")
}

#[test]
fn both_agents_receive_the_emitted_skill_and_repeat_install_preserves_it() {
    let home = TempDir::new().expect("temporary home");
    let shown = run(&home, &["skill", "show"]);
    assert!(shown.status.success());
    for _ in 0..2 {
        assert!(run(&home, &["skill", "install"]).status.success());
    }
    for root in [".claude", ".agents"] {
        let dir = home.path().join(root).join("skills/switchbard");
        assert_eq!(
            fs::read(dir.join("SKILL.md")).expect("installed skill"),
            shown.stdout
        );
        assert!(dir.join("agents/openai.yaml").is_file());
    }
    assert!(!home.path().join(".switchbard").exists());
}

#[test]
fn custom_skill_conflict_preserves_both_destinations_before_writing() {
    let home = TempDir::new().expect("temporary home");
    let target = home.path().join(".agents/skills/switchbard");
    fs::create_dir_all(&target).expect("custom skill directory");
    fs::write(target.join("SKILL.md"), "User custom instructions").expect("custom skill");
    assert!(!run(&home, &["skill", "install"]).status.success());
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md")).expect("custom preserved"),
        "User custom instructions"
    );
    assert!(!home.path().join(".claude").exists());
    assert!(run(&home, &["skill", "install", "--replace"])
        .status
        .success());
    assert_eq!(
        fs::read(target.join("SKILL.md")).expect("explicit replacement"),
        run(&home, &["skill", "show"]).stdout
    );
}

#[test]
fn single_agent_install_does_not_touch_other_agent() {
    let home = TempDir::new().expect("temporary home");
    assert!(run(&home, &["skill", "install", "--agent", "codex"])
        .status
        .success());
    assert!(!home.path().join(".claude").exists());
    assert!(home
        .path()
        .join(".agents/skills/switchbard/SKILL.md")
        .is_file());
}

#[test]
fn replace_still_refuses_symlinks_and_preserves_target() {
    let home = TempDir::new().expect("temporary home");
    let outside = TempDir::new().expect("outside destination");
    fs::write(outside.path().join("keep"), "untouched").expect("outside fixture");
    symlink(outside.path(), home.path().join(".claude")).expect("symlink fixture");
    assert!(!run(&home, &["skill", "install", "--replace"])
        .status
        .success());
    assert_eq!(
        fs::read_to_string(outside.path().join("keep")).expect("unchanged"),
        "untouched"
    );
    assert!(!outside.path().join("skills").exists());
    assert!(!home.path().join(".agents").exists());
}

#[test]
fn setup_prompt_can_be_printed_without_creating_any_configuration() {
    let home = TempDir::new().expect("temporary home");
    let output = run(&home, &["agent-prompt"]);
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        fs::read_dir(home.path()).expect("unchanged home").count(),
        0
    );
}
