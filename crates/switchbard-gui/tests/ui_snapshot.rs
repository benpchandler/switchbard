//! Opt-in pixel snapshot of the Agent Context view, for visual-regression work.
//!
//! This is `#[ignore]`d on purpose. It renders through `wgpu` (a real GPU
//! adapter) and compares against a committed PNG baseline — which is sensitive
//! to GPU/driver/font differences across machines, so it is **not** part of the
//! CI gate. CI still *compiles* it (catching API breakage); it just doesn't run
//! it. The accesskit interaction tests in `ui_views.rs` are the durable,
//! deterministic layer.
//!
//! Workflow when you want visual regression locally — create/refresh the
//! baseline, then validate against it:
//!
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test -p switchbard-gui --test ui_snapshot -- --ignored
//! cargo test -p switchbard-gui --test ui_snapshot -- --ignored
//! ```
//!
//! Baselines land in `tests/snapshots/<name>.png`; the `.new.png` / `.diff.png`
//! outputs are gitignored.

mod common;

use common::{harness, seeded_app};
use switchbard_gui::runtime::ViewTab;

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn agent_context_view_snapshot() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::AgentContext;
    let mut harness = harness(app);
    harness.run();
    harness.wgpu_snapshot("agent_context_view");
}
