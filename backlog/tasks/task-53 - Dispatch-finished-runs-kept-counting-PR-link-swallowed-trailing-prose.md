---
id: TASK-53
title: 'Dispatch: finished runs kept counting; PR link swallowed trailing prose'
status: Done
assignee: []
created_date: '2026-08-26 00:40'
updated_date: '2026-08-26 00:42'
labels: []
dependencies: []
priority: high
ordinal: 53000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two defects found by dogfooding the Dispatches tab. (1) DispatchRun::elapsed always measured to now, so LED-580 - a 30-minute run that ended five days earlier - rendered as 'ran 132h 33m' under Awaiting review, a section that by definition only holds finished runs. A finished run's clock now stops at log_modified_unix, which --output-format text makes effectively the agent's exit time; a log stamped before the start falls back to now rather than claiming zero duration. (2) find_note_suffix took the rest of the line after 'Dispatch PR: ', so prose a human appended after the URL became part of it - render_outcome uses that value as both a hyperlink's label and its target, giving a wrapped wall of blue text AND a link that opened nothing. Now takes the first whitespace-delimited token.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A finished run reports its own duration, not time since it started
- [x] #2 A live run keeps counting to now
- [x] #3 A PR note with trailing prose yields only the URL
- [x] #4 Both regressions confirmed to fail under sabotage
<!-- AC:END -->
