---
name: Weekly goals
status: Planned
---
Weekly numeric goals with tracking relative to target (owner request 2026-08-31): 'onboard 5 users this week' vs 4 actually onboarded. Two goal kinds: task-derived (actual computed from done-in-week tasks matching a scope) and manual-metric (actual from dated, append-only check-in observations; current derived from the last entry). STORAGE (owner decision 2026-08-31): NOT markdown - goals are records, not documents. One structured backlog/goals.yml per repo: goals list, each with unit/measure/scope and a weeks map of {target, checkins:[{date,value}]}. Line-surgical YAML edits through the core write layer (precedent: status_config.rs on config.yml). Cross-week history is one read; 'goal roll' adds a week key. The load-bearing derived signal is PACE: actual/target vs elapsed_days/7 -> on-track / behind / met / missed. Deferred, ask first: auto-recurring templates. Format divergence: trajectory-doc entry before code.
