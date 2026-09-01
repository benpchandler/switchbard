---
name: Stack Ranking
status: Completed
---
Manual stack rank within the repo: rank siblings within their parent scope (projects in repo, tasks in project, sub-issues in parent), sparse with computed-comparator fallback; repo-wide next-up order is computed by flattening, never stored; a short expedite lane holds true cross-project interrupts. Storage is backlog/ranking.yml (records, not documents) owned by backlog/ranking.rs - renamed from the planned ordering.yml, which the hub's cross-repo triage overlay already owns. Design record: docs/product-trajectory.md 'Stack ranking' entry (owner-approved 2026-08-31). Shipped 2026-09-01 (PRs #66, #67).
