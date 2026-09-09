---
name: Bugs
status: In Progress
---

Standing bucket for defects filed from the tools themselves: sbt's `:bug` routes every report here (crates/switchbard-tui/src/report.rs, BUG_PROJECT) so a filed defect is never loose in the backlog. Unlike a shipped-and-done project this one never completes; it is a triage queue, and its roll-up percentage reads as "how much of the known defect load is cleared", not "how close to release". Groom a filed report before dispatching it: the filer sets priority medium and one confirmation AC by default, and both usually need replacing.
