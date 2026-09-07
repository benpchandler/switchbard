---
name: Views That Keep Themselves
status: Planned
---

sbt asks the user to know, in advance, what they will want later - and they never do. Named slots need foresight at build time; paint_auto freezes a palette choice as if it were hand-authored. Both push the cost of a decision onto the moment where the user has the least information.

Telemetry over 45h (~/.switchbard/tui-events.jsonl, 2026-09-02..04, 135 sessions): 355 view-mutating actions produced 2 saves, across 207 session ends of which 6 were a deliberate quit. Of 28 paint color decisions, 16 were paint_auto and 0 were a hex the owner typed - yet the saved view is entirely hex.

The three tasks here move sbt from recall to recognition: store what was actually decided rather than what it resolved to (152), keep the last arrangement without being asked (153), and let the user find an old one by looking at it instead of remembering it (154). Sequenced in that order - a history that stores resolved hex would resurrect stale palettes on every restore.

Found 2026-09-04 while installing the Darkroom lights-out theme, when a saved paint rule silently overrode the new theme and the cause was invisible from the screen.
