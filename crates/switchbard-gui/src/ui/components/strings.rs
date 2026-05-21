//! Shared user-facing copy. Anything that appears in column headers, hover
//! tooltips, or empty-state messages should live here so wording stays
//! consistent across views.

pub const COL_BRANCH: &str = "BRANCH";
pub const COL_HEAD: &str = "HEAD";
pub const COL_STATUS: &str = "STATUS";
pub const COL_DRIFT: &str = "DRIFT";
pub const COL_LAST_COMMIT: &str = "LAST COMMIT";
pub const COL_ACTIVITY: &str = "ACTIVITY";
pub const COL_LISTENERS: &str = "LISTENERS";
pub const COL_PATH: &str = "PATH";

pub const COL_SERVICE: &str = "SERVICE";
pub const COL_STATE: &str = "STATE";
pub const COL_PORTS: &str = "PORTS";
pub const COL_ACTIONS: &str = "ACTIONS";
pub const COL_COMMAND: &str = "COMMAND";

pub const COL_PORT: &str = "PORT";
pub const COL_PID: &str = "PID";
pub const COL_PGID: &str = "PGID";
pub const COL_CWD: &str = "CWD";
pub const COL_ACTION: &str = "ACTION";
pub const COL_REPO: &str = "REPO";

pub const HOVER_DRIFT_HEADER: &str = "How far this branch has diverged from its upstream remote. \
     '+N/-M' means N commits ahead of origin and M behind. \
     '—' = in sync (or no upstream set); '…' = probe pending.";

pub const HOVER_ACTIVITY_HEADER: &str = "Recent commit velocity on this branch. \
     Burst = 3+ commits in the last 30min (agent hammering away); \
     Active = at least one in the last hour; \
     Slow = something in the last 24h; \
     Idle = nothing recent. Hover the cell to see the subjects.";
