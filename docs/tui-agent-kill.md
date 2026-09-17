# Selected agent termination (TASK-246)

## Objective and boundaries

Enable sending SIGTERM to the selected Agents-tab process without leaving the TUI. Uppercase `K` is configurable as `kill_agent`; lowercase `k` remains navigation. Both list and detail use the same action. Confirmation defaults to Cancel and requires Enter on the signal option after the complete identity and warning have rendered. Termination can lose unsaved work. Only the positive selected PID is signalled, never its process group; children may remain. No task edits or manual claim release occur. Live claims clear through the existing normal Pruned lifecycle when the agent exits, exactly as with a manual PID kill.

Core owns the safety boundary, including for a second frontend: a precise OS start token, agent executable kind and cwd must match the observed row at preparation and again at execution. Invalid, self, ancestor, stale and unverifiable identities fail closed. Preparation and execution run off the input thread, without an unbounded registry subprocess or automatic escalation. A successful syscall means signal sent, not process exited. Both platforms have a residual race between the final native identity read and positive-PID signal; this implementation does not use an atomic process handle. Native Claude package `node_modules/@anthropic-ai/claude-code/bin/claude.exe` is recognized narrowly in the kill boundary; this does not widen the scan's process-name catalog. Linux's kernel ` (deleted)` executable annotation is stripped only for classification, retaining the full path for identity equality. Listed-only or unsupported wrapped executables without a native scan identity refuse explicitly; at most 512 rows receive native identity probes per poll, with excess rows retained without signal permission.

Worker owns code, tests and this evidence in `feat/tui-agent-kill`; command owns tracker, integration, guarded installation and independent audit. No live user process may be signalled in testing. Real subprocess fixtures are unit-owned, harmless and reaped. Native owner acceptance remains separate from automated evidence.

## State and stress contract

| State / stress | Required outcome | Evidence gate |
| --- | --- | --- |
| Default, empty, list and open detail | K discoverable in hints/help; empty gives feedback; k navigates | Real key/render harness |
| Preparing, submitting, repeated K / Enter | Responsive input; one bounded operation; no duplicate signal | Async TUI journey |
| Confirmation, cancellation, reload or selection change | Cancel selected initially; Esc/Enter-Cancel signals nothing; immutable target revalidated | Real child survival and harness |
| Success / still alive / already gone | Honest signal-sent or already-gone feedback; agent refresh; no task edits/manual release; normal Pruned cleanup | Isolated child and neighbor |
| Failure, stale, permission-limited, access removed | Visible reason; no unsafe signal; can retry new observation | Core identity/refusal tests; permission fixture gap if unavailable |
| Invalid PID, self, ancestor, wrong executable/cwd/start | Public core API refuses, including forged or stale caller data | Real OS probes and focused tests |
| Narrow40x8, current100x24, wide160x40, tiny | Complete warning/identity required to permit; tiny confirmation blocked | Real render matrix |
| Long / unbroken / Unicode names and cwd, duplicates, many | No ambiguity; PID/kind/cwd remain confirmation authority; bounded rendering | Harness fixtures |
| Remapped action, keyboard focus, interruption | Configuration/help agree; explicit Enter only; no hidden confirmation permit | Key/remap tests |
| Pointer/touch and zoom | No new pointer action; terminal cell layout governs; existing mouse navigation unchanged | N/A new surface; existing suite |
| Historical/read-only/task conflict | No task write or archive, independent from claims/task lifecycle | Fixture record comparison |
| Offline / unavailable listing | Per-PID native probe does not invoke registry/network | Native identity implementation and tests |
| Linux parity | Exact /proc start ticks, executable and cwd; positive PID signal | Linux CI gap until run |

## Evidence ledger

Actionable observations rebase process kind, cwd and displayed start from one native capture with precise birth reads before/after cwd/executable acquisition. Earlier discovery fields are PID hints, not identity authority. Merge preserves native birth/kind/cwd. Registry names, session IDs and activity remain observational labels, not proven birth associations, and confirmation warns they may be stale. A concrete kind/cwd mismatch or birth difference beyond 2 seconds disables capability; the 2-second allowance accommodates coarse ps elapsed rounding/read timing and does not establish identity. Final preparation/execution equality uses the precise native token, never this allowance. There remains registry association ambiguity for same-kind/cwd fast replacements; Enter authorizes the fully displayed native PID identity, not the session label. Fleet pgid dedup metadata remains unchanged and is never used for signalling.

The real E2E fixture is a compiled harmless native C process named `claude`, with no children, a 60-second alarm and optional SIGTERM ignore. Copying macOS system executables initially produced SIGKILL 9 independently of the operation; exact exit-signal assertions exposed that false fixture and it was replaced rather than weakening the checks. Tests now prove signal 15 exits the selected child, a shared-group neighbor survives, an ignoring child remains alive and normal Pruned history occurs without Released/Passed or task-record changes. A real read-only owner probe (`cargo run -q -p switchbard-core --example agent_identity_probe -- 52374`) recognized Claude at exact birth 1789589903:105782 with the owner's native package executable and CambridgeKitchens cwd; it invoked no preparation, termination or signal.

Before implementation, `cargo test -p switchbard-tui --test agent_kill -- --nocapture` reproduced the missing action in the actual 100x20 E2E container: `K is not bound. ? lists keys`. Cold build 11.60s; one expected failing test. Initial worktree was clean; only the owned reproduction test was added. A subsequent real harness reproduction exposed unavailable identities getting stuck on Checking; its new journey verifies immediate refusal with no pending operation or signal.

Final macOS gates: workspace formatting check, core/TUI all-targets clippy, full TUI tests and full core tests pass. TUI runner summaries aggregate 495 passed and 22 ignored across 72 result groups; core summaries aggregate 760 passed and 2 ignored across 9 result groups. Ten feature journeys in `crates/switchbard-tui/tests/agent_kill.rs` verify the materially different containers and transitions above. Three real-OS focused tests in `crates/switchbard-core/tests/agent_kill.rs` prove public safety guards, native-authoritative capture/merge and neighboring process survival. `cargo check -p switchbard-gui --tests` passes in 21.22s, verifying all existing GUI session literals remain compatible. Prior interrupted full gates are not counted as accepted evidence.

Explicit gaps: native owner interaction/visual acceptance, Linux execution/CI, real permission-denied and thread-quota failures, hostile Unicode/long cwd variants and interruption by installed-binary reload have not been separately proved. Core stale-discovery fixtures use a real process with injected contradictory discovery metadata; they do not force kernel PID recycling. The final read-to-signal race and observational registry-label association ambiguity remain as described above. Existing registry listing timeouts are not redesigned by this feature.
