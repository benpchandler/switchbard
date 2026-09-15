//! Persistence failure and retention journeys through real keys and rendered UI.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use switchbard_tui::views::ViewState;

const BYTE_CAP: usize = 4 * 1024 * 1024;
const AGE: u64 = 30 * 24 * 60 * 60;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

fn entry(h: &Harness, filter: &str, visited_at: u64) -> Value {
    let view = ViewState {
        filter: filter.into(),
        ..Default::default()
    };
    json!({"page":"tasks", "visited_at":visited_at, "lua":view.to_lua(h.app.registry())})
}

fn seed_history(h: &Harness, entries: Vec<Value>) {
    fs::write(
        h.root.join("views-repo.history.json"),
        serde_json::to_vec(&json!({"version":1,"entries":entries})).expect("fixture JSON"),
    )
    .expect("history fixture");
}

fn history(h: &Harness) -> Vec<Value> {
    let document: Value = serde_json::from_slice(
        &fs::read(h.root.join("views-repo.history.json")).expect("history bytes"),
    )
    .expect("history JSON");
    document["entries"].as_array().expect("entries").clone()
}

fn reopen(h: &mut Harness) {
    h.app = open_app(&h.root, &h.config_path);
    h.app.restore_session(None, false);
    h.render();
}

fn filter(h: &mut Harness, text: &str) {
    h.press(KeyCode::Esc);
    h.type_text("/");
    h.type_text(text);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.filter, text);
}

fn checkpoint(h: &mut Harness, sequence: u64) {
    h.app
        .checkpoint_if_due(Instant::now() + Duration::from_secs(31 * sequence));
    h.render();
}

fn task_entries(h: &Harness) -> Vec<Value> {
    history(h)
        .into_iter()
        .filter(|entry| entry["page"] == "tasks")
        .collect()
}

#[test]
fn returning_views_redate_once_and_cold_reopen_restores_their_full_arrangement() {
    let mut h = Harness::new();
    seed_history(
        &h,
        vec![
            entry(&h, "login", now() - 3600),
            entry(&h, "theme", now() - 7200),
        ],
    );
    reopen(&mut h);
    filter(&mut h, "login");
    let capture_started = now();
    checkpoint(&mut h, 1);
    filter(&mut h, "theme");
    checkpoint(&mut h, 2);
    filter(&mut h, "login");
    checkpoint(&mut h, 3);
    let entries = task_entries(&h);
    assert_eq!(entries.len(), 2);
    assert!(entries[0]["lua"].as_str().expect("Lua").contains("login"));
    assert!(entries[0]["visited_at"].as_u64().expect("timestamp") >= capture_started);
    checkpoint(&mut h, 4);
    assert_eq!(
        task_entries(&h),
        entries,
        "idle checkpoint does not rewrite history"
    );
    h.type_text("vh");
    let screen = h.render();
    assert!(
        screen.contains("login") && screen.contains("just now"),
        "{screen}"
    );
    h.press(KeyCode::Enter);
    assert!(h.render().contains("Fix login redirect loop"));
    let mut aged = task_entries(&h);
    aged[0]["visited_at"] = json!(now() - 60);
    seed_history(&h, aged);
    reopen(&mut h);
    h.press(KeyCode::Down);
    let capture_started = now();
    checkpoint(&mut h, 1);
    assert!(
        task_entries(&h)[0]["visited_at"]
            .as_u64()
            .expect("timestamp")
            >= capture_started
    );
    assert!(h.render().contains("Fix login redirect loop"));
}

#[test]
fn count_and_age_caps_keep_recent_views_and_renew_the_current_expired_view() {
    let mut h = Harness::new();
    let time = now();
    seed_history(
        &h,
        (0..1000)
            .map(|i| entry(&h, &format!("old{i}"), time - 2000 + i))
            .collect(),
    );
    reopen(&mut h);
    filter(&mut h, "login");
    checkpoint(&mut h, 1);
    assert_eq!(history(&h).len(), 1000);
    assert!(!history(&h)
        .iter()
        .any(|e| e["lua"].as_str().expect("Lua").contains("\"old0\"")));
    h.type_text("vhlogin");
    assert!(h.render().contains("login"));
    h.press(KeyCode::Enter);
    assert!(h.render().contains("Fix login redirect loop"));
    seed_history(
        &h,
        vec![
            entry(&h, "login", time - AGE - 1),
            entry(&h, "expired", time - AGE - 2),
        ],
    );
    reopen(&mut h);
    checkpoint(&mut h, 1);
    let retained = task_entries(&h);
    assert_eq!(retained.len(), 1);
    assert!(retained[0]["lua"].as_str().expect("Lua").contains("login"));
    h.type_text("vh");
    let screen = h.render();
    assert!(
        screen.contains("login") && !screen.contains("expired"),
        "{screen}"
    );
}

#[test]
fn mixed_size_history_evicts_oldest_small_views_then_restores_from_disk() {
    let mut h = Harness::new();
    let time = now() - 2000;
    let entries = (0..900)
        .map(|i| entry(&h, &format!("small{i}"), time + i))
        .chain((0..67).map(|i| {
            entry(
                &h,
                &format!("large{i}{}", "x".repeat(60_000)),
                time + 1000 + i,
            )
        }))
        .collect();
    seed_history(&h, entries);
    assert!(
        fs::metadata(h.root.join("views-repo.history.json"))
            .expect("fixture")
            .len()
            <= BYTE_CAP as u64
    );
    let large = ViewState {
        filter: format!("new{}", "x".repeat(60_000)),
        ..Default::default()
    };
    fs::write(
        h.root.join("views-repo.lua"),
        format!("return {{ [2] = {} }}", large.to_lua(h.app.registry())),
    )
    .expect("saved large view");
    reopen(&mut h);
    h.type_text("v2vp");
    let started = Instant::now();
    checkpoint(&mut h, 1);
    eprintln!("mixed-size App checkpoint: {:?}", started.elapsed());
    assert!(
        fs::metadata(h.root.join("views-repo.history.json"))
            .expect("history")
            .len()
            <= BYTE_CAP as u64
    );
    let entries = task_entries(&h);
    assert!(entries.len() < 967 && entries.len() > 68);
    assert_eq!(
        entries
            .iter()
            .filter(|e| e["visited_at"].as_u64().expect("time") >= time + 1000)
            .count(),
        68
    );
    assert!(entries
        .windows(2)
        .all(|pair| pair[0]["visited_at"].as_u64() >= pair[1]["visited_at"].as_u64()));
    reopen(&mut h);
    h.type_text("v1vhnew");
    assert!(h.render().contains("new"));
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.filter, large.filter);
    assert!(!h.app.state.pin_top);
}

#[test]
fn invalid_future_and_oversized_records_are_visible_and_preserved_after_keys() {
    for (extension, source) in [
        ("history.json", "{".into()),
        ("history.json", r#"{"version":2,"entries":[]}"#.into()),
        ("history.json", "x".repeat(BYTE_CAP + 1)),
        ("resume", "sbt-resume-9={}".into()),
        ("resume", "x".repeat(1_048_577)),
    ] {
        let mut h = Harness::new();
        let path = h.root.join(format!("views-repo.{extension}"));
        fs::write(&path, &source).expect("invalid fixture");
        reopen(&mut h);
        let screen = h.render();
        assert!(
            screen.contains("history") || screen.contains("resume"),
            "{screen}"
        );
        filter(&mut h, "login");
        checkpoint(&mut h, 1);
        assert!(h.render().contains("not saved"));
        assert_eq!(fs::read_to_string(&path).expect("preserved"), source);
        assert!(h.render().contains("Fix login redirect loop"));
    }
}

#[test]
fn concurrent_sessions_preserve_the_first_write_and_the_second_live_view() {
    let mut first = Harness::new();
    let second_app = open_app(&first.root, &first.config_path);
    filter(&mut first, "login");
    checkpoint(&mut first, 1);
    let history_bytes =
        fs::read(first.root.join("views-repo.history.json")).expect("first history");
    let resume_bytes = fs::read(first.root.join("views-repo.resume")).expect("first resume");
    first.app = second_app;
    filter(&mut first, "theme");
    checkpoint(&mut first, 1);
    let screen = first.render();
    assert!(
        screen.contains("not saved") && screen.contains("Add dark theme"),
        "{screen}"
    );
    assert_eq!(
        fs::read(first.root.join("views-repo.history.json")).expect("history"),
        history_bytes
    );
    assert_eq!(
        fs::read(first.root.join("views-repo.resume")).expect("resume"),
        resume_bytes
    );
    assert_eq!(first.app.state.filter, "theme");
}

struct Writer(Child);
impl Drop for Writer {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill() {
            eprintln!("writer kill: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("writer reap: {error}");
        }
    }
}

fn lock_writer(h: &Harness) -> Writer {
    let mut child = Command::new("python3").args(["-c", "import fcntl,sys,time\nfiles=[open(p,'a+') for p in sys.argv[1:]]\nfor f in files: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)\nprint('ready',flush=True)\ntime.sleep(20)"])
        .arg(h.root.join("views-repo.history.history.lock"))
        .arg(h.root.join("views-repo.resume.lock"))
        .stdout(Stdio::piped()).spawn().expect("real OS lock holder");
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("ready line");
    assert_eq!(line.trim(), "ready");
    Writer(child)
}

#[test]
fn killed_writer_and_stale_temporary_files_do_not_block_recovery() {
    let mut h = Harness::new();
    for suffix in [
        "history.history.pending".to_string(),
        "history.history.1.0.0.tmp".into(),
        format!("resume.{}.tmp", std::process::id()),
    ] {
        fs::write(
            h.root.join(format!("views-repo.{suffix}")),
            "interrupted writer",
        )
        .expect("stale temp");
    }
    let writer = lock_writer(&h);
    filter(&mut h, "login");
    checkpoint(&mut h, 1);
    assert!(h.render().contains("not saved"));
    assert!(!h.root.join("views-repo.resume").exists());
    assert!(!h.root.join("views-repo.history.json").exists());
    h.type_text("vh");
    assert!(h.render().contains("No view history yet"));
    h.press(KeyCode::Esc);
    drop(writer);
    checkpoint(&mut h, 2);
    reopen(&mut h);
    assert!(h.render().contains("Fix login redirect loop"));
    h.type_text("vh");
    assert!(h.render().contains("login"));
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.filter, "login");
}

#[test]
fn malformed_lua_is_bounded_and_visible_without_executing_external_commands() {
    const CHILD: &str = "SBT_PERSISTENCE_PARSER_JOURNEY";
    if std::env::var_os(CHILD).is_none() {
        let mut child = Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "malformed_lua_is_bounded_and_visible_without_executing_external_commands",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bounded E2E child");
        // The bound proves the parser *terminates* (its own limit is ten
        // thousand Lua instructions, microseconds); the budget is generous
        // because the child's five harness reopens run beside every other
        // test in this binary on a loaded machine, where ten seconds flaked.
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            if child.try_wait().expect("child status").is_some() {
                let output = child.wait_with_output().expect("child output");
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        child.kill().expect("stop unbounded parser");
        child.wait().expect("reap parser");
        panic!("parser prevented an E2E child from responding within sixty seconds");
    }
    for record in [
        "(function() while true do end end)()".to_string(),
        "(function() local s='x';for i=1,24 do s=s..s end;return {filter=s} end)()".into(),
        "(function() os.execute('false');return {filter='login'} end)()".into(),
        "{future=true}".into(),
        format!("{{filter='{}'}}", "x".repeat(65_536)),
    ] {
        let mut h = Harness::new();
        let path = h.root.join("views-repo.history.json");
        let source = serde_json::to_vec(
            &json!({"version":1,"entries":[{"page":"tasks","visited_at":now(),"lua":record}]}),
        )
        .expect("fixture");
        fs::write(&path, &source).expect("adversarial source");
        reopen(&mut h);
        assert!(h.render().contains("view history"));
        filter(&mut h, "login");
        checkpoint(&mut h, 1);
        assert!(h.render().contains("not saved"));
        assert_eq!(fs::read(&path).expect("preserved source"), source);
        assert!(h.render().contains("Fix login redirect loop"));
    }
}

#[test]
fn another_repository_still_opens_its_default_after_a_checkpoint() {
    let mut first = Harness::new();
    filter(&mut first, "login");
    checkpoint(&mut first, 1);
    let mut second = Harness::new();
    reopen(&mut second);
    second.press(KeyCode::Down);
    let screen = second.render();
    assert!(screen.contains("Add dark theme") && screen.contains("Write onboarding guide"));
    assert!(second.app.state.filter.is_empty());
    assert!(!second.root.join("views-repo.resume").exists());
    assert!(!second.root.join("views-repo.history.json").exists());
}

#[test]
fn an_oversized_saved_arrangement_preserves_usable_resume_and_history() {
    let mut h = Harness::new();
    filter(&mut h, "login");
    checkpoint(&mut h, 1);
    let resume_path = h.root.join("views-repo.resume");
    let usable_resume = fs::read(&resume_path).expect("usable resume");
    let large = ViewState {
        filter: "x".repeat(65_536),
        ..Default::default()
    };
    fs::write(
        h.root.join("views-repo.lua"),
        format!("return {{ [2] = {} }}", large.to_lua(h.app.registry())),
    )
    .expect("oversized saved view");
    reopen(&mut h);
    h.type_text("v2vp");
    checkpoint(&mut h, 1);
    assert!(h.render().contains("not saved"));
    assert!(
        fs::read(&resume_path).expect("resume after refusal") == usable_resume,
        "oversized live arrangement must not replace previous usable resume"
    );
    assert!(task_entries(&h)
        .iter()
        .all(|entry| entry["lua"].as_str().expect("Lua").len() < 65_536));
    let cold_app = open_app(&h.root, &h.config_path);
    let invalid_app = std::mem::replace(&mut h.app, cold_app);
    h.app.restore_session(None, false);
    assert!(h.render().contains("Fix login redirect loop"));
    assert_eq!(h.app.state.filter, "login");
    h.app = invalid_app;
    // Returning to a valid arrangement can checkpoint in the same process.
    filter(&mut h, "theme");
    checkpoint(&mut h, 2);
    reopen(&mut h);
    assert!(h.render().contains("Add dark theme"));
    assert_eq!(h.app.state.filter, "theme");
    h.type_text("vhlogin");
    assert!(h.render().contains("login"));
    h.press(KeyCode::Enter);
    assert!(h.render().contains("Fix login redirect loop"));
}
