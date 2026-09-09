use super::*;
use crate::storage::MigrationPlan;
use tempfile::{tempdir, TempDir};

struct Peer {
    _root: TempDir,
    store: Store,
    repo: RepositoryId,
}

fn origin() -> Peer {
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = store.bind_repository(root.path()).unwrap();
    store
        .apply_migration(
            &MigrationPlan::capture(
                repo.clone(),
                vec!["task".into(), "initiative".into()],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    store
        .mutate(&repo, "task", "1", None, |_| Ok(Some(b"one".to_vec())))
        .unwrap();
    store
        .mutate(&repo, "task", "2", None, |_| Ok(Some(b"two".to_vec())))
        .unwrap();
    Peer {
        _root: root,
        store,
        repo,
    }
}
fn clone_peer(snapshot: &ExchangeSnapshot) -> Peer {
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = store.bind_exchange(root.path(), snapshot).unwrap();
    Peer {
        _root: root,
        store,
        repo,
    }
}
fn export(peer: &Peer) -> ExchangeSnapshot {
    peer.store.export_snapshot(&peer.repo).unwrap()
}
fn edit(peer: &mut Peer, locator: &str, text: &str) {
    peer.store
        .mutate(&peer.repo, "task", locator, None, |_| {
            Ok(Some(text.as_bytes().to_vec()))
        })
        .unwrap();
}
fn import(peer: &mut Peer, snapshot: &ExchangeSnapshot) -> Result<ImportResult> {
    let preview = peer.store.preview_import(&peer.repo, snapshot)?;
    peer.store
        .apply_import(&peer.repo, snapshot, preview.expected_sequence)
}

#[test]
fn bootstrap_edit_export_back_and_empty_kind() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    assert!(b.store.authority(&b.repo, "initiative").unwrap());
    edit(&mut b, "1", "from b");
    import(&mut a, &export(&b)).unwrap();
    assert_eq!(export(&a), export(&b));
}

#[test]
fn skipped_exports_and_alternating_imports() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    for version in 0..5 {
        edit(&mut a, "1", &version.to_string());
        export(&a);
    }
    import(&mut b, &export(&a)).unwrap();
    for version in 0..3 {
        edit(&mut b, "2", &format!("b{version}"));
        import(&mut a, &export(&b)).unwrap();
        edit(&mut a, "1", &format!("a{version}"));
        import(&mut b, &export(&a)).unwrap();
    }
    assert_eq!(export(&a), export(&b));
}

#[test]
fn offline_disjoint_edits_converge() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    edit(&mut a, "1", "a");
    edit(&mut b, "2", "b");
    let a_snapshot = export(&a);
    import(&mut a, &export(&b)).unwrap();
    import(&mut b, &a_snapshot).unwrap();
    assert_eq!(export(&a), export(&b));
}

#[test]
fn same_record_conflict_is_atomic_and_explicit_resolution_converges() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    edit(&mut a, "1", "a");
    edit(&mut b, "1", "b");
    edit(&mut b, "2", "also b");
    let before = export(&a);
    let incoming = export(&b);
    assert!(import(&mut a, &incoming).is_err());
    assert_eq!(before, export(&a));
    let preview = a.store.preview_import(&a.repo, &incoming).unwrap();
    assert_eq!(preview.conflicts.len(), 1);
    let resolutions =
        BTreeMap::from([(preview.conflicts[0].clone(), ConflictResolution::Incoming)]);
    a.store
        .apply_import_with_resolutions(&a.repo, &incoming, preview.expected_sequence, &resolutions)
        .unwrap();
    import(&mut b, &export(&a)).unwrap();
    assert_eq!(export(&a), export(&b));
}

#[test]
fn stale_snapshot_and_omission_never_roll_back_or_delete() {
    let mut a = origin();
    let stale = export(&a);
    edit(&mut a, "1", "new");
    let current = export(&a);
    import(&mut a, &stale).unwrap();
    assert_eq!(export(&a), current);
    let mut omitted = current.clone();
    omitted.records.clear();
    omitted.refresh_digest().unwrap();
    import(&mut a, &omitted).unwrap();
    assert_eq!(export(&a), current);
}

#[test]
fn edit_tombstone_conflicts_and_tombstone_dominates_stale_replay() {
    let mut a = origin();
    let stale = export(&a);
    let mut b = clone_peer(&stale);
    a.store
        .mutate(&a.repo, "task", "1", None, |_| Ok(None))
        .unwrap();
    edit(&mut b, "1", "edit");
    assert!(import(&mut a, &export(&b)).is_err());
    import(&mut a, &stale).unwrap();
    assert!(a.store.read(&a.repo, "task", "1").unwrap().unwrap().deleted);
}

#[test]
fn equal_clock_different_content_and_wrong_epoch_reject() {
    let mut a = origin();
    let original = export(&a);
    let mut changed = original.clone();
    changed.records[0].content = ExchangeContent::from_bytes(b"forged");
    changed.refresh_digest().unwrap();
    assert!(import(&mut a, &changed).is_err());
    let mut epoch = original.clone();
    epoch.epoch_id = Uuid::new_v4().to_string();
    epoch.refresh_digest().unwrap();
    assert!(import(&mut a, &epoch).is_err());
    assert_eq!(export(&a), original);
}

#[test]
fn restored_database_rotates_replica_and_divergent_changes_conflict() {
    let mut a = origin();
    let backup_root = tempdir().unwrap();
    let path = backup_root.path().join("db");
    a.store.backup_to(&path).unwrap();
    let mut b = Peer {
        _root: backup_root,
        store: Store::open(&path).unwrap(),
        repo: a.repo.clone(),
    };
    b.store.rotate_replica().unwrap();
    edit(&mut a, "1", "a");
    edit(&mut b, "1", "restored b");
    let preview = a.store.preview_import(&a.repo, &export(&b)).unwrap();
    assert_eq!(preview.conflicts.len(), 1);
}

#[test]
fn concurrent_equal_content_coalesces_and_replay_is_idempotent() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    edit(&mut a, "1", "same");
    edit(&mut b, "1", "same");
    import(&mut a, &export(&b)).unwrap();
    import(&mut b, &export(&a)).unwrap();
    assert_eq!(export(&a), export(&b));
    let sequence = a.store.change_sequence().unwrap();
    assert_eq!(import(&mut a, &export(&b)).unwrap().changed_records, 0);
    assert_eq!(a.store.change_sequence().unwrap(), sequence);
}

#[test]
fn readable_unknown_content_and_binary_round_trip() {
    let mut a = origin();
    let source = a._root.path().join("future.raw");
    std::fs::write(&source, [0xff, 0x00]).unwrap();
    a.store
        .apply_migration(
            &MigrationPlan::capture(
                a.repo.clone(),
                vec!["future-kind".into()],
                vec![crate::storage::SourceDocument {
                    kind: "future-kind".into(),
                    locator: "opaque".into(),
                    path: source,
                }],
            )
            .unwrap(),
        )
        .unwrap();
    edit(
        &mut a,
        "1",
        "---\ncustom: {nested: [null, '', {}, []]}\n---\n## Custom\nUnicode 🐝",
    );
    let snapshot = export(&a);
    let bytes = snapshot.to_bytes().unwrap();
    assert!(std::str::from_utf8(&bytes)
        .unwrap()
        .contains("custom: {nested:"));
    let parsed = ExchangeSnapshot::parse(&bytes).unwrap();
    let b = clone_peer(&parsed);
    assert_eq!(export(&b), snapshot);
}

#[test]
fn stale_preview_and_record_locator_collision_have_no_effect() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    edit(&mut b, "1", "incoming");
    let incoming = export(&b);
    let preview = a.store.preview_import(&a.repo, &incoming).unwrap();
    edit(&mut a, "2", "local");
    assert!(a
        .store
        .apply_import(&a.repo, &incoming, preview.expected_sequence)
        .is_err());
    let mut collision = export(&a);
    collision.records[0].id = Uuid::new_v4().to_string();
    collision.refresh_digest().unwrap();
    assert!(import(&mut a, &collision).is_err());
}

#[test]
fn separate_local_bindings_can_explicitly_share_repository_identity() {
    let a = origin();
    let mut b = clone_peer(&export(&a));
    let second = b._root.path().join("second");
    std::fs::create_dir(&second).unwrap();
    assert_eq!(b.store.bind_exchange(&second, &export(&a)).unwrap(), a.repo);
    assert_eq!(b.store.repository(&second).unwrap(), Some(a.repo));
}

#[test]
fn domain_validator_failure_rolls_back_import_and_binding() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    edit(&mut b, "1", "invalid domain content");
    let incoming = export(&b);
    let before = export(&a);
    let sequence = a.store.change_sequence().unwrap();
    assert!(a
        .store
        .apply_import_checked(
            &a.repo,
            &incoming,
            sequence,
            &BTreeMap::new(),
            |_| anyhow::bail!("domain rejection")
        )
        .is_err());
    assert_eq!(export(&a), before);
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    assert!(store
        .bind_exchange_checked(root.path(), &incoming, |_| anyhow::bail!(
            "domain rejection"
        ))
        .is_err());
    assert!(store.repository(root.path()).unwrap().is_none());
}

#[test]
fn utf8_lines_preserve_final_newline_crlf_and_reject_ambiguous_fragments() {
    for bytes in [b"".as_slice(), b"one", b"one\n", b"one\r\n\nlast"] {
        assert_eq!(ExchangeContent::from_bytes(bytes).bytes().unwrap(), bytes);
    }
    assert!(ExchangeContent::Utf8Lines(vec!["one".into(), "two".into()])
        .bytes()
        .is_err());
    assert!(ExchangeContent::Utf8Lines(vec!["one\ntwo\n".into()])
        .bytes()
        .is_err());
    assert!(ExchangeContent::Utf8Lines(vec![String::new()])
        .bytes()
        .is_err());
}

#[test]
fn duplicate_outer_and_clock_keys_and_counter_exhaustion_reject() {
    let mut a = origin();
    let original = export(&a);
    let wire = String::from_utf8(original.to_bytes().unwrap()).unwrap();
    assert!(ExchangeSnapshot::parse(
        wire.replacen("\"version\": 2", "\"version\": 2, \"version\": 2", 1)
            .as_bytes()
    )
    .is_err());
    let replica = original.records[0].clock.keys().next().unwrap();
    let duplicate = wire.replacen(
        &format!("\"{replica}\": 1"),
        &format!("\"{replica}\": 1, \"{replica}\": 1"),
        1,
    );
    assert!(ExchangeSnapshot::parse(duplicate.as_bytes()).is_err());
    let record = original
        .records
        .iter()
        .find(|record| record.locator == "1")
        .unwrap();
    let exhausted = BTreeMap::from([(replica.clone(), 9_007_199_254_740_991)]);
    clocks::save(&a.store.connection, &record.id, &exhausted).unwrap();
    let before = export(&a);
    assert!(a
        .store
        .mutate(&a.repo, "task", "1", None, |_| Ok(Some(
            b"overflow".to_vec()
        )))
        .is_err());
    assert_eq!(export(&a), before);
}

#[test]
fn offline_creation_collision_requires_explicit_relocation_and_preserves_both() {
    let mut a = origin();
    let mut b = clone_peer(&export(&a));
    edit(&mut a, "3", "created a");
    edit(&mut b, "3", "created b");
    let incoming = export(&b);
    let preview = a.store.preview_import(&a.repo, &incoming).unwrap();
    assert_eq!(preview.conflicts.len(), 1);
    let before = export(&a);
    assert!(import(&mut a, &incoming).is_err());
    assert_eq!(before, export(&a));
    let resolutions = BTreeMap::from([(
        preview.conflicts[0].clone(),
        ConflictResolution::RelocateIncoming {
            locator: "4".into(),
            content: b"created b with new public identifier".to_vec(),
        },
    )]);
    a.store
        .apply_import_with_resolutions(&a.repo, &incoming, preview.expected_sequence, &resolutions)
        .unwrap();
    import(&mut b, &export(&a)).unwrap();
    assert_eq!(export(&a), export(&b));
    assert_eq!(a.store.list(&a.repo, "task").unwrap().len(), 4);
}

#[test]
fn bind_preview_sequence_is_rechecked_under_writer_lock() {
    let a = origin();
    let snapshot = export(&a);
    let root = tempdir().unwrap();
    let target = root.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let sequence = store.change_sequence().unwrap();
    store.bind_repository(root.path()).unwrap();
    assert!(store
        .bind_exchange_checked_at(&target, &snapshot, sequence, |_| Ok(()))
        .is_err());
    assert!(store.repository(&target).unwrap().is_none());
}

#[test]
fn json_nesting_has_an_explicit_32_level_boundary() {
    let accepted = format!("{}0{}", "[".repeat(32), "]".repeat(32));
    assert!(validate_json_depth(accepted.as_bytes()).is_ok());
    let rejected = format!("{}0{}", "[".repeat(33), "]".repeat(33));
    assert!(validate_json_depth(rejected.as_bytes()).is_err());
    assert!(ExchangeSnapshot::parse(rejected.as_bytes())
        .unwrap_err()
        .to_string()
        .contains("depth"));
    assert!(validate_json_depth(br#"{"text":"[[[[[]]]]]\"{"}"#).is_ok());
}

#[test]
fn opaque_versions_round_trip_and_local_mutations_refuse_without_effects() {
    let a = origin();
    let mut future = export(&a);
    let mut unknown = future.records[0].clone();
    unknown.id = Uuid::new_v4().to_string();
    unknown.kind = "future-kind".into();
    unknown.locator = "opaque-custom".into();
    unknown.content_version = 17;
    unknown.content = ExchangeContent::from_bytes(&[0xff, 0x01]);
    future.records[1].content_version = 2;
    future.records[1].content = ExchangeContent::from_bytes(b"opaque not v1 Markdown");
    future.kinds.push("future-kind".into());
    future.records.push(unknown);
    future.refresh_digest().unwrap();
    let future = ExchangeSnapshot::parse(&future.to_bytes().unwrap()).unwrap();
    let mut b = clone_peer(&future);
    let opaque_before: Vec<_> = export(&b)
        .records
        .into_iter()
        .filter(|record| record.content_version > 1)
        .collect();
    let known = future
        .records
        .iter()
        .find(|record| record.kind == "task" && record.content_version == 1)
        .unwrap();
    edit(&mut b, &known.locator, "known edit");
    let after = export(&b);
    assert_eq!(
        after
            .records
            .iter()
            .filter(|record| record.content_version > 1)
            .cloned()
            .collect::<Vec<_>>(),
        opaque_before
    );
    let sequence = b.store.change_sequence().unwrap();
    for record in after
        .records
        .iter()
        .filter(|record| record.content_version > 1)
    {
        assert!(b
            .store
            .mutate(&b.repo, &record.kind, &record.locator, None, |_| Ok(Some(
                b"bad edit".to_vec()
            )))
            .is_err());
        assert!(b
            .store
            .mutate_kind(&b.repo, &record.kind, None, |documents| {
                documents
                    .iter_mut()
                    .find(|document| document.id == record.id)
                    .unwrap()
                    .content = b"bad edit".to_vec();
                Ok(())
            })
            .is_err());
    }
    assert_eq!(b.store.change_sequence().unwrap(), sequence);
    assert_eq!(export(&b), after);
    let c = clone_peer(&ExchangeSnapshot::parse(&after.to_bytes().unwrap()).unwrap());
    assert_eq!(export(&c), after);
}

#[test]
fn legacy_version_one_wire_omits_content_version_and_keeps_digest_compatibility() {
    let a = origin();
    let snapshot = export(&a);
    let wire = snapshot.to_bytes().unwrap();
    assert!(!String::from_utf8_lossy(&wire).contains("content_version"));
    let parsed = ExchangeSnapshot::parse(&wire).unwrap();
    assert!(parsed
        .records
        .iter()
        .all(|record| record.content_version == 1));
    assert_eq!(parsed.digest, snapshot.digest);
}
