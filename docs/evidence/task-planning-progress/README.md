# Planning and checklist render evidence

Captured from the real TUI renderer (`ratatui::TestBackend`) using synthetic task files and real key events. Temporary directory names were replaced with `fixture`; no live repository task data was used. These are terminal cell buffers, not native font screenshots.

- `wide-mixed-review.txt`: 140 x 28, Planned order followed by other tasks; Considering and Planned independent of execution status; completed checklist requests Review; zero criteria are Unmeasured.
- `narrow-mixed-review.txt`: 60 x 14, identical records; column text clips within each cell while Review remains visible first.
- `detail.txt`: 140 x 28, selected record detail includes editable Planning, combined task/descendant checklist, and separate acceptance criteria.

Behavior evidence lives in `crates/switchbard-tui/tests/planning_progress.rs`: fresh defaults and existing saved columns; menu planning edits and reload; full coverage without automatic Done; historical Done with incomplete criteria; stale detail refusal; ordering/filtering/grouping/sorting; descendant weighting; explicit cancellation route; empty narrow state. `tests/detail_central.rs` also verifies Planning updates with the retained task files removed and central storage authoritative.

Local terminal surfaces have no touch, web zoom, account-role, or remote-loading states. Native terminal font appearance is not proved by these buffers. Existing native detail, pointer, sections, and scrolling tests retain their scope.
