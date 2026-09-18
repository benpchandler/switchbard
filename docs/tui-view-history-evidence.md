# Last view and recognizable history (TASK-153 / TASK-154)

## Objective and decisions

Complete cold-start resumption and automatic recognizable history, including the palette-token prerequisite (TASK-152). The implementation worktree started clean at f8098a30. The primary checkout's pre-existing pickers.rs, view.rs and filter.rs edits are excluded.

Always resume the latest checkpoint on a cold start; `--fresh` opens the deliberate saved default. Keep the existing ResumeRecord and Lua ViewState representation. Committed arrangement and page changes checkpoint resume immediately, while selection-only navigation and filter drafts do not write it. Capture history every 30 seconds, including while a picker is open, and on graceful exit or handled SIGHUP/SIGTERM/SIGINT through one App checkpoint method. Forced termination retains the latest completed checkpoint. History retains 30 days and at most 1000 distinct arrangements across both list pages, with a 4 MiB file and 64 KiB entry bound. Resume has a 1 MiB bound. History is separate from deliberate slots, per repo and page. A restored history view is promoted through existing `v s <number>`.

## Acceptance evidence

| Requirement | Executed behavior / authority |
| --- | --- |
| 153.1: cold launch restores per repo | `tests/cold_resume.rs` round-trips every view field, both pages and selection through the actual App; `tests/signal_resume.rs` launches the real binary repeatedly under a PTY with isolated HOME and repo. Relative `--repo .` and default cwd share one record. |
| 153.2: same self-restart representation | `App::resume_state` produces the existing `sbt-resume-1` ResumeRecord; both environment handoff and `.resume` file consume it. Codec tests retain older named records and legacy positional records. |
| 153.3: deliberate defaults stay untouched | Cold/default/fresh and PTY tests inspect persisted slot output and prove automatic capture does not create or change repo/global `.lua` slots. |
| 153.4: first-ever launch | Cold-start test with no resume file opens the deliberately saved default. |
| 153.5: documented opt-out | Real PTY launches verify `--fresh` clears the prior session filter; rendered `?` help advertises history and fresh launch; `docs/tui-view-history.md` documents semantics and self-restart precedence. |
| 153.6: prompt arrangement resume | Real PTY second-session coverage verifies that a committed arrangement is visible within the checkpoint latency budget, without waiting for the 30-second history cadence; selection-only navigation and filter drafts remain non-persistent. |
| 154.1: one automatic capture path | `App::tick` checks the deadline; timer and every graceful exit call `checkpoint_session`. App deadline tests verify capture without saving and while the paint picker remains open. Real PTY test keeps a stable copied binary alive through the actual 30-second deadline, verifies both durable files while it is running, then sends SIGKILL and verifies cold restoration; it also covers SIGHUP/SIGTERM/SIGINT. |
| 154.2: global dedupe / move to front | `tests/persistence_edges.rs` uses persisted fixtures and real keys for A/B/A, idle capture, cold revisit, expiry and reload; one arrangement moves to front and is re-dated, with no duplicate for names or cursor positions. |
| 154.3: recognizable cards, labels and time | `tests/history_recognition.rs`, `tests/history_preview.rs` and `tests/view_history.rs` render actual menus with each visible entry's relative age, plain-language title and miniature table, including independent filter/sort/group/column/glyph/row layout descriptions and paint assignments. PgUp/PgDn pages cards without restoring. |
| 154.4: restore and promote | Real keys `v h`, Enter, `v s 2` restore and save; reopening proves the slot is durable. Picker payloads retain the serialized entry, so a concurrent capture cannot redirect selection to a different entry. |
| 154.5 / prerequisite 152: current palette | Paint tests reproduce and fix literal hex corruption, assert auto `p<n>` storage, current-palette cell/picker rendering, reload, shorter/empty palettes, and legacy hex retention. History tests restore token-backed paint after a palette change. Explicit literals remain literal. |
| 154.6: bounded retention | `tests/persistence_edges.rs` covers 30-day expiry, active renewal, 1000 global entries, 4 MiB file trimming, 64 KiB entry rejection and bounded input parsing. Limits are documented. |

## State and stress matrix

| Dimension | Evidence and result |
| --- | --- |
| Default, first run, fresh, cold/restart precedence, resumption | Cold-start, codec, real CLI launch and rendered help tests passed. |
| Active and changed views, immediate resume, timer, exit, repeated capture, A/B/A, independent pages | App/session tests passed; committed arrangements checkpoint immediately, while picker mode does not starve the 30-second history timer. |
| Empty, one, many, 1000 entries, expired records and byte ceiling | Persisted-fixture App journeys and actual rendered menu tests passed. |
| Corrupt/future files, failed writes, busy lock, external edits, retry, killed writer | Sources and prior in-memory history are preserved. OS locks release on process death; unique temporary names avoid abandoned-file blockage. `tests/persistence_edges.rs` drives real keys and rendered errors against malformed fixtures, concurrent Apps and a separate lock-holding process. |
| Oversized live view | The real-key saved-view journey reproduces the rejected-history/overwritten-resume mismatch, then verifies outbound shared validation preserves the previous usable resume and allows later valid edits to save. |
| Long labels, multiline/control characters, unbroken strings and non-Latin text | Full-arrangement cold restore and adversarial persisted Lua journeys passed; render tests cover Japanese, accented Latin and long unbroken strings. |
| Narrow/current/wide/short/zero terminal | Rendered at 40x8, 80x24, 120x40, 180x50, 20x4/20x5, and zero dimensions. Long selected entries wrap and scroll at 40x8; tiny containers degrade safely. |
| Keyboard selection, search, no matches, cancel, restore, deliberate slot save | Real key tests passed; typing does not restore prematurely. Arrow navigation and PgUp/PgDn remain available. |
| Navigation while capturing / stable identity | Capture after opening/searching history does not change the selected entry's serialized payload. |
| Palette change / old literals / auto tokens | Paint and history tests passed. No reverse palette lookup remains. |
| Performance at cap | Review reproduced repeated whole-document serialization while evicting small entries. Capture now budgets exact serialized entry lengths, trims by subtraction and encodes the final document once. A mixed 900-small/67-large-entry regression verifies exact byte limits; observed debug capture fell from a worst 44.43 seconds to about 180 milliseconds. This is a local observation, not a timing threshold in CI. |
| Network, remote permissions, offline/reconnect | N/A to automatic persistence, which uses local files; remote PR operations are unchanged. |
| Pointer, touch, browser zoom | N/A to the existing keyboard terminal interface. |
| Loading/saving progress | No asynchronous save state. Local capture is bounded; errors are visible in status and on exit. |

## Scope and delivery gates

Fact-source checks: all automatic triggers use `App::checkpoint_session`; resume and history reuse the same `ViewState` Lua codec; all token rendering uses `paint::resolve_color`. A second list consumer does not implement its own view serializer, palette reverse lookup or history capture logic.

Local targeted suites pass. Full workspace preflight and final delivery review are tracked separately from these scoped results. The captured history-card renders were reviewed separately from TestBackend or PTY success; installation, CI and merge are not implied. No shared app was installed or restarted. Extremely small terminals cannot display a complete preview; widening the terminal or scrolling the supported preview reveals it.
