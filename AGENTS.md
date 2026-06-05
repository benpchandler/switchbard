# AGENTS.md

## Render-path perf

When touching egui render paths (`crates/switchbard-gui/src/app.rs` or `crates/switchbard-gui/src/ui/**`), run a quick perf smoke before calling the work done. Use `SWITCHBARD_PERF=1` with `SWITCHBARD_PERF_LOG=/tmp/switchbard-perf.csv`, exercise Servers scrolling, and compare p95 frame/workspace time against the previous build. Avoid full snapshot rebuilds, per-row clones, and unbounded file lists in per-frame UI code.
