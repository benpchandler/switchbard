# Date painting and shared paint evaluation

`p` offers **When task filed** on Tasks and **When merged** on Pull Requests, including when the field is hidden or no rows currently match. The existing value/color picker applies rules; saved spellings are `filed` (`created` alias) and `merged` (`merged_at` alias). Text filters use compact tokens such as `filed:last7days` or `merged:missing`.

## Date contract

UTC calendar days are the comparison boundary. Backlog's authoritative `created_date` uses its existing UTC `YYYY-MM-DD HH:MM` convention. GitHub's authoritative `mergedAt` is parsed once into `DateTime<Utc>` and accepted only for a Merged PR. Closing, completing, updating or editing a file never substitutes for these timestamps.

| Category | UTC day difference from today |
| --- | --- |
| today | 0 |
| yesterday | 1 |
| last 7 days | 0 through 6, inclusive |
| last 30 days | 0 through 29, inclusive |
| older | 30 or more |
| future | Negative difference |
| missing | Absent or invalid timestamp; all unmerged PRs |

Ranges overlap intentionally. The first matching value in a by-column rule retains the existing within-rule precedence; the lowest matching rule in the rule hierarchy wins a cell, and the top rule is the whole-row base. Independent row rules can compose date filters with other filters. Date categories update when rendered/evaluated; relative tokens, not frozen dates, persist in existing view files. The compact cell uses the first applicable bucket. Categories are not groupable because a date can match several ranges.

## Semantic boundary

`paint_eval.rs` selects opaque color tokens from shared column values and the existing `Filter` matcher. It does not import Ratatui or convert color strings. `paint.rs` retains the existing configuration parser and terminal color conversion. Tasks and PRs reach this single precedence evaluator through explicit field adapters. No UI dependency was added to core. Existing task by-value rules retain their first-category behavior for labels/goals; dates deliberately provide all overlapping buckets. Palette-token storage and recoloring remain in their original authority.

## Evidence

- `tests/date_paint.rs`: disk-backed real-key journeys at 42/80/140 columns, empty filter result, future/today/yesterday/6/7/29/30-day boundaries, invalid calendar dates, invalid strings, absent dates, overlap and row/column precedence, Tasks/PR scope and slot restart.
- `tests/paint.rs`: existing seven real-key precedence, palette, clear/reorder and save/reload journeys.
- `tests/date_paint.rs::authoritative_live_merge_dates_paint_real_pr_rows_and_survive_restart`: executed with authenticated read-only GitHub history from this repository, verified a real merged row's timestamp and rendered green cell, then restarted and reloaded its PR rule.
- Core PR parser tests: RFC3339 timezone offset normalization, missing/null/invalid values, and contradictory non-merged lifecycle.
- GUI and touch/browser scaling: N/A, frontend-specific terminal controls only. Terminal widths and keyboard navigation cover this interaction model.
- Human appearance approval is not inferred from rendered buffers or PTY interaction.
