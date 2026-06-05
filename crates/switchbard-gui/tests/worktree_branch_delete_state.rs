//! Dialog-state logic for the opt-in branch cleanup in the remove-worktree
//! confirmation. The core git facts are tested in `switchbard-core`; these tests
//! pin the GUI's decisions about *when to offer* deletion and *whether the
//! confirmed action deletes the branch*.

use std::path::PathBuf;

use switchbard_core::BranchDeleteAssessment;
use switchbard_gui::runtime::ConfirmRemoveWorktree;

fn dialog(
    branch: Option<&str>,
    assessment: Option<BranchDeleteAssessment>,
    delete_branch: bool,
) -> ConfirmRemoveWorktree {
    ConfirmRemoveWorktree {
        repo_path: PathBuf::from("/repo"),
        worktree_path: PathBuf::from("/repo/.worktrees/feat"),
        branch: branch.map(str::to_string),
        dirty_files: vec![],
        active_runs: vec![],
        branch_assessment: assessment,
        delete_branch,
        busy: false,
        error: None,
    }
}

fn landed(branch: &str) -> BranchDeleteAssessment {
    BranchDeleteAssessment {
        branch: branch.to_string(),
        other_checkouts: vec![],
        unmerged_commits: Some(0),
        compared_against: Some("main".into()),
    }
}

fn unlanded(branch: &str, n: u32) -> BranchDeleteAssessment {
    BranchDeleteAssessment {
        branch: branch.to_string(),
        other_checkouts: vec![],
        unmerged_commits: Some(n),
        compared_against: Some("main".into()),
    }
}

fn blocked(branch: &str) -> BranchDeleteAssessment {
    BranchDeleteAssessment {
        branch: branch.to_string(),
        other_checkouts: vec![PathBuf::from("/repo")],
        unmerged_commits: Some(0),
        compared_against: Some("main".into()),
    }
}

#[test]
fn detached_head_offers_no_branch_delete() {
    let d = dialog(None, None, false);
    assert!(!d.can_offer_branch_delete());
    assert!(!d.will_delete_branch());
}

#[test]
fn landed_branch_is_offered_and_honored_when_checked() {
    let d = dialog(Some("feat/foo"), Some(landed("feat/foo")), true);
    assert!(d.can_offer_branch_delete());
    assert!(d.will_delete_branch());
    assert!(!d.branch_assessment.as_ref().unwrap().needs_force());
}

#[test]
fn unchecked_box_means_no_branch_delete_even_when_offered() {
    let d = dialog(Some("feat/foo"), Some(landed("feat/foo")), false);
    assert!(d.can_offer_branch_delete());
    assert!(!d.will_delete_branch());
}

#[test]
fn unlanded_branch_is_offered_but_marked_force() {
    let d = dialog(Some("feat/foo"), Some(unlanded("feat/foo", 3)), true);
    assert!(d.can_offer_branch_delete());
    assert!(d.will_delete_branch());
    assert!(d.branch_assessment.as_ref().unwrap().needs_force());
    assert_eq!(d.branch_assessment.as_ref().unwrap().unmerged_count(), 3);
}

#[test]
fn branch_checked_out_elsewhere_is_never_offered() {
    // Even if the user's prior state had the box checked, a blocked assessment
    // must not result in a deletion.
    let d = dialog(Some("main"), Some(blocked("main")), true);
    assert!(!d.can_offer_branch_delete());
    assert!(!d.will_delete_branch());
}
