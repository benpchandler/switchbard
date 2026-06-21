use switchbard_gui::worktree_actions::removal_error_message;

#[test]
fn no_services_killed_returns_git_error_verbatim() {
    let msg = removal_error_message(0, "git worktree remove failed: is dirty");
    assert_eq!(msg, "git worktree remove failed: is dirty");
}

#[test]
fn one_service_killed_uses_singular_wording() {
    let msg = removal_error_message(1, "git worktree remove failed: is dirty");
    assert_eq!(
        msg,
        "stopped 1 service, but removal failed: git worktree remove failed: is dirty"
    );
}

#[test]
fn multiple_services_killed_uses_plural_wording() {
    let msg = removal_error_message(3, "git worktree remove failed: is dirty");
    assert_eq!(
        msg,
        "stopped 3 services, but removal failed: git worktree remove failed: is dirty"
    );
}
