use super::{Check, Status};
use std::path::Path;
use switchbard_core::storage::{inspect_database, DatabaseInspection, WorkspaceInspection};

pub(super) fn check(database: Option<&Path>, root: Option<&Path>, checks: &mut Vec<Check>) {
    checks.push(Check::new(
        "sqlite",
        true,
        Status::Ok,
        "SQLite is embedded in Switchbard. No database installation or server is needed.",
        None,
    ));
    let Some(database) = database else {
        checks.push(Check::new(
            "database",
            true,
            Status::Error,
            "Database destination cannot be resolved.",
            Some("Check your home directory and SWITCHBARD_DATABASE; its value must not be empty."),
        ));
        return;
    };
    match inspect_database(database, root) {
        Ok(DatabaseInspection::Missing | DatabaseInspection::Empty) => {
            check_destination(database, checks);
            checks.push(Check::new("workspace", false, Status::Warning,
                "Database setup has not run. Doctor did not create it.",
                Some("Run sbt in your repository and accept setup, or run sbt init --yes to use suggested defaults.")));
        }
        Ok(DatabaseInspection::UpgradeRequired) => {
            check_destination(database, checks);
            checks.push(Check::new("workspace", false, Status::Warning,
                "Existing Switchbard database needs its supported schema upgrade. Doctor did not change it.",
                Some("Back up your database, then run sbt normally to apply its supported upgrade.")));
        }
        Ok(DatabaseInspection::WalInspectionSkipped) => checks.push(Check::new(
            "database", true, Status::Error,
            "Database uses WAL mode; read-only inspection was skipped to avoid creating SQLite sidecars. Database health and workspace registration remain unverified.",
            Some("Back up the database with its WAL files, then use the owning sb storage status workflow to diagnose it. Native Switchbard normally uses DELETE journal mode."))),
        Ok(DatabaseInspection::Ready(workspace)) => {
            check_destination(database, checks);
            checks.push(workspace_check(workspace));
        }
        Err(_) => checks.push(Check::new("database", true, Status::Error,
            "Existing storage is invalid, unavailable, has unsafe permissions, or has a repository identity conflict. Doctor did not modify it.",
            Some("Run sb storage status for the owning storage error. Restore a missing/corrupt established database from backup; do not delete its marker or reinitialize it."))),
    }
}

fn workspace_check(workspace: WorkspaceInspection) -> Check {
    match workspace {
        WorkspaceInspection::Central => Check::new("workspace", false, Status::Ok, "Repository workspace is configured in the central database.", None),
        WorkspaceInspection::Registered => Check::new("workspace", false, Status::Warning,
            "Repository has retained legacy storage authority.", Some("Use sb storage status and the reviewed migration workflow; doctor does not migrate data.")),
        WorkspaceInspection::Unconfigured => Check::new("workspace", false, Status::Warning,
            "This repository has no central workspace registration.", Some("Run sbt for setup. If legacy backlog data exists, use sb storage status before attempting migration.")),
    }
}

fn check_destination(database: &Path, checks: &mut Vec<Check>) {
    let writable = writable_destination(database);
    checks.push(Check::new("database", true, if writable { Status::Ok } else { Status::Error },
        if writable { "Database destination has writable owner permissions. No probe file was created; quotas and ACLs remain unchecked." }
        else { "Database destination is not accessible with writable owner permissions." },
        (!writable).then_some("Choose a user-owned writable destination with SWITCHBARD_DATABASE, or repair directory permissions. Doctor does not change permissions.")));
}

fn writable_destination(database: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Some(home) = dirs::home_dir().and_then(|path| std::fs::metadata(path).ok()) else {
        return false;
    };
    if database.exists()
        && std::fs::OpenOptions::new()
            .write(true)
            .open(database)
            .is_err()
    {
        return false;
    }
    let mut parent = database
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    while !parent.exists() {
        let Some(ancestor) = parent.parent() else {
            return false;
        };
        parent = ancestor;
    }
    std::fs::metadata(parent).is_ok_and(|metadata| {
        metadata.is_dir() && metadata.uid() == home.uid() && metadata.mode() & 0o300 == 0o300
    })
}
