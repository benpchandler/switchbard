---
name: Stack Ranking
status: Planned
---

Manual stack rank within the repo: rank siblings within their parent scope (projects in repo, tasks in project, sub-issues in parent), sparse with computed-comparator fallback; repo-wide next-up order is computed by flattening, never stored; a short expedite lane holds true cross-project interrupts. Storage is backlog/ordering.yml (records, not documents) owned by backlog/ordering.rs. Design record: docs/product-trajectory.md 'Stack ranking' entry (owner-approved 2026-08-31).
