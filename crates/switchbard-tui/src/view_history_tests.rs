use super::*;

fn state(filter: &str) -> ViewState {
    ViewState {
        filter: filter.into(),
        ..ViewState::default()
    }
}

#[test]
fn returning_arrangement_moves_to_front_without_idle_refresh() {
    let registry = ColumnRegistry::builtin_only();
    let (mut store, _) = HistoryStore::load(None, &registry);
    for (filter, now) in [("a", 1), ("b", 2), ("a", 3), ("a", 4)] {
        store
            .capture(Page::Tasks, &state(filter), &registry, now)
            .expect("capture");
    }
    let entries = store.entries(Page::Tasks, 4);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].visited_at, 3);
    assert_eq!(entries[0].view(&registry).expect("view").filter, "a");
}

#[test]
fn durable_reload_restores_full_state_and_separates_pages() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.json");
    let registry = ColumnRegistry::builtin_only();
    let (mut store, _) = HistoryStore::load(Some(path.clone()), &registry);
    let mut view = state("status:todo");
    view.name = "saved name".into();
    view.pin_top = false;
    view.row_layout.spaced = true;
    view.row_layout.cycle_wrapping();
    store
        .capture(Page::Tasks, &view, &registry, 10)
        .expect("tasks");
    store
        .capture(
            Page::PullRequests,
            &ViewState::pull_requests(),
            &registry,
            11,
        )
        .expect("prs");
    let (mut reloaded, warnings) = HistoryStore::load(Some(path), &registry);
    assert!(warnings.is_empty());
    view.name.clear();
    assert_eq!(
        reloaded.entries(Page::Tasks, 12)[0]
            .view(&registry)
            .expect("view"),
        view
    );
    assert_eq!(reloaded.entries(Page::PullRequests, 12).len(), 1);
    reloaded
        .capture(Page::Tasks, &view, &registry, 12)
        .expect("idle");
    assert_eq!(reloaded.entries(Page::Tasks, 12)[0].visited_at, 12);
    reloaded
        .capture(Page::Tasks, &view, &registry, 13)
        .expect("idle");
    assert_eq!(reloaded.entries(Page::Tasks, 13)[0].visited_at, 12);
}

#[test]
fn age_and_global_count_are_bounded_and_active_arrangement_is_retained() {
    let registry = ColumnRegistry::builtin_only();
    let (mut store, _) = HistoryStore::load(None, &registry);
    for index in 0..MAX_ENTRIES + 20 {
        store
            .capture(
                Page::Tasks,
                &state(&index.to_string()),
                &registry,
                index as u64,
            )
            .expect("capture");
    }
    assert_eq!(store.entries(Page::Tasks, 2000).len(), MAX_ENTRIES);
    let now = MAX_AGE_SECONDS + 3000;
    assert!(store.entries(Page::Tasks, now).is_empty());
    for time in [now, now + 1] {
        store
            .capture(
                Page::Tasks,
                &state(&(MAX_ENTRIES + 19).to_string()),
                &registry,
                time,
            )
            .expect("expiry");
        assert_eq!(store.entries(Page::Tasks, time).len(), 1);
        assert_eq!(store.entries(Page::Tasks, time)[0].visited_at, now);
    }
}

#[test]
fn corruption_future_version_and_external_changes_preserve_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.json");
    let registry = ColumnRegistry::builtin_only();
    for bad in [
        "{",
        r#"{"version":2,"entries":[]}"#,
        r#"{"version":1,"entries":[{"page":"tasks","visited_at":1,"lua":"{future=true}"}]}"#,
    ] {
        fs::write(&path, bad).expect("source");
        let (mut store, warnings) = HistoryStore::load(Some(path.clone()), &registry);
        assert!(!warnings.is_empty());
        assert!(store
            .capture(Page::Tasks, &state("a"), &registry, 2)
            .is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), bad);
    }
    fs::remove_file(&path).expect("remove");
    let (mut store, _) = HistoryStore::load(Some(path.clone()), &registry);
    fs::write(&path, "external edit").expect("external");
    assert!(store
        .capture(Page::Tasks, &state("a"), &registry, 2)
        .is_err());
    assert_eq!(fs::read_to_string(&path).expect("read"), "external edit");
    assert!(store.entries(Page::Tasks, 2).is_empty());
}

#[test]
fn byte_cap_and_oversized_entry_are_enforced() {
    let registry = ColumnRegistry::builtin_only();
    let (mut store, _) = HistoryStore::load(None, &registry);
    for index in 0..90 {
        store
            .capture(
                Page::Tasks,
                &state(&format!("{index}{}", "x".repeat(60_000))),
                &registry,
                index,
            )
            .expect("capture");
    }
    assert!(store.entries.len() < 90);
    assert!(encode(&store.entries).expect("encode").len() <= MAX_FILE_BYTES);
    assert!(store
        .capture(
            Page::Tasks,
            &state(&"x".repeat(MAX_ENTRY_BYTES)),
            &registry,
            100
        )
        .is_err());
}

#[test]
fn shared_parser_rejects_execution_and_unknown_fields() {
    let registry = ColumnRegistry::builtin_only();
    for text in [
        "(function() while true do end end)()",
        "os.execute('false')",
        "{future = true}",
        "not lua",
    ] {
        assert!(ViewState::try_from_lua(text, &registry).is_err(), "{text}");
    }
    assert!(ViewState::try_from_lua("{ filter = 'a' }", &registry).is_ok());
}

#[test]
fn competing_writer_and_failed_write_leave_memory_unchanged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.json");
    let registry = ColumnRegistry::builtin_only();
    let (mut first, _) = HistoryStore::load(Some(path.clone()), &registry);
    let (mut second, _) = HistoryStore::load(Some(path.clone()), &registry);
    first
        .capture(Page::Tasks, &state("first"), &registry, 1)
        .expect("first");
    assert!(second
        .capture(Page::Tasks, &state("second"), &registry, 2)
        .is_err());
    assert!(second.entries(Page::Tasks, 2).is_empty());
    let guard = lock_history(&path).expect("other writer");
    assert!(first
        .capture(Page::Tasks, &state("new"), &registry, 3)
        .is_err());
    drop(guard);
    assert_eq!(
        first.entries(Page::Tasks, 3)[0]
            .view(&registry)
            .expect("view")
            .filter,
        "first"
    );
}

#[test]
fn stale_pending_files_do_not_block_a_new_checkpoint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.json");
    fs::write(
        path.with_extension("history.pending"),
        "interrupted old writer",
    )
    .expect("old pending");
    fs::write(
        path.with_extension("history.1.0.0.tmp"),
        "interrupted writer",
    )
    .expect("old temp");
    fs::write(path.with_extension("history.lock"), "").expect("stale lock");
    let registry = ColumnRegistry::builtin_only();
    let (mut store, _) = HistoryStore::load(Some(path.clone()), &registry);
    store
        .capture(Page::Tasks, &state("new session"), &registry, 1)
        .expect("checkpoint");
    let (reloaded, warnings) = HistoryStore::load(Some(path), &registry);
    assert!(warnings.is_empty());
    assert_eq!(reloaded.entries(Page::Tasks, 1).len(), 1);
}

#[test]
fn crash_lock_holder() {
    let Some(path) = std::env::var_os("SBT_HISTORY_CRASH_TEST") else {
        return;
    };
    let path = PathBuf::from(path);
    let _guard = lock_history(&path).expect("child lock");
    fs::write(path.with_extension("ready"), "ready").expect("child ready");
    std::thread::sleep(std::time::Duration::from_secs(10));
}

#[test]
fn killed_writer_releases_lock_for_retry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.json");
    let mut child = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "view_history::tests::crash_lock_holder"])
        .env("SBT_HISTORY_CRASH_TEST", &path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("child writer");
    for _ in 0..100 {
        if path.with_extension("ready").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ready = path.with_extension("ready").exists();
    let registry = ColumnRegistry::builtin_only();
    let (mut store, _) = HistoryStore::load(Some(path), &registry);
    let blocked = store.capture(Page::Tasks, &state("a"), &registry, 1);
    child.kill().expect("kill writer");
    child.wait().expect("reap writer");
    assert!(ready, "child acquired OS lock");
    assert!(blocked.expect_err("busy writer").contains("busy"));
    store
        .capture(Page::Tasks, &state("a"), &registry, 2)
        .expect("retry after crash");
    assert_eq!(store.entries(Page::Tasks, 2)[0].visited_at, 2);
}

#[test]
fn mixed_size_history_evicts_only_oldest_entries_to_exact_json_budget() {
    let registry = ColumnRegistry::builtin_only();
    let (mut store, _) = HistoryStore::load(None, &registry);
    for index in 0..900 {
        store.entries.push(HistoryEntry {
            page: HistoryPage::Tasks,
            visited_at: index,
            lua: state(&format!("small{index}")).to_lua(&registry),
        });
    }
    for index in 0..67 {
        store.entries.push(HistoryEntry {
            page: HistoryPage::Tasks,
            visited_at: 1000 + index,
            lua: state(&format!("large{index}{}", "x".repeat(60_000))).to_lua(&registry),
        });
    }
    store.entries.reverse();
    assert!(encode(&store.entries).expect("before").len() <= MAX_FILE_BYTES);
    let newest = state(&format!("new{}", "x".repeat(60_000)));
    let started = std::time::Instant::now();
    store
        .capture(Page::Tasks, &newest, &registry, 2000)
        .expect("capture");
    eprintln!("mixed-size capture: {:?}", started.elapsed());
    assert!(store.entries.len() < 967, "multiple small entries evicted");
    assert!(
        store.entries.len() > 68,
        "retains small entries that still fit"
    );
    assert_eq!(store.entries[0].view(&registry).expect("newest"), newest);
    assert_eq!(
        store
            .entries
            .iter()
            .filter(|e| e.visited_at >= 1000)
            .count(),
        68
    );
    assert!(store
        .entries
        .windows(2)
        .all(|pair| pair[0].visited_at > pair[1].visited_at));
    let output = encode(&store.entries).expect("output");
    assert!(output.len() <= MAX_FILE_BYTES);
    assert_eq!(
        decode(&output, &registry).expect("reload").len(),
        store.entries.len()
    );
}
