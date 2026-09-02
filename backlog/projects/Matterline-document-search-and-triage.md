---
name: Matterline document search and triage
status: Planned
---

## Outcome

Make Matterline document search a fast, trustworthy triage surface for known correspondents and recurring patterns. The Inspiration Wharf golden case is finding all emails involving Forchelli, adding the right messages to a durable Need to read queue, and acting on a result set without changing evidentiary conclusions by accident.

## Product problem

The production screen currently searches broadly when a user types Forchelli, hides participant search under Filters, presents multiple unexplained counts, keeps attachments mixed into the working set, and leaves a dense single-document coding rail open during set triage. The selected row can also say UNREVIEWED while the rail says Reviewed and All changes saved.

## Decisions

- Make participant intent discoverable next to the general search field. Participant matching covers From, To, Cc, and Bcc; ordinary search remains whole-document search.
- Explain scope and counts explicitly: matching emails versus related attachments or inline items. Bulk selection defaults to matching emails.
- Model Need to read as a durable attention queue, separate from Reviewed, relevance, importance, privilege, and highlights.
- Use a reversible Hide from this pass action for culling. Do not introduce an Unimportant coding value; Background or Not relevant remain deliberate review judgments.
- Treat rules as named, versioned search/filter criteria with preview and explicit Apply. Do not silently auto-code review conclusions in the first release.
- Resolve the review-state contradiction: the row, rail, and saved-state indicator must represent the same persisted state.

## First vertical slice

1. Typing Forchelli offers Email participants: Forchelli and makes the active scope visible.
2. The result header reports email matches and related items with a clear inclusion toggle.
3. Select matching emails exposes a bulk action tray: Add to Need to read and Hide from this pass.
4. Save as rule stores the criteria and shows a preview count; Apply is manual and auditable.
5. Opening one document returns to the compact review form; the Activity surface must not obscure the reading or action area.

## Acceptance

- A browser journey finds all Forchelli participant emails across From, To, Cc, and Bcc, including partial structured metadata supplemented by raw headers.
- Participant filters combine with text search and every visible filter using AND semantics; comma-separated participant terms use OR semantics.
- Bulk actions state their exact scope, are idempotent, and record actor, criteria, matched count, excluded count, and timestamp.
- Need to read and Hide from this pass are reversible and do not mark a document Reviewed or Not relevant.
- Review-state changes require an explicit document-level action and never appear saved while the row remains unreviewed.
- The first slice passes against real PostgreSQL and a Chromium journey using the Inspiration Wharf golden case.

## Non-goals

No silent event-driven auto-rules, no bulk privilege decisions, no automatic Reviewed or Not relevant labels from a pattern, and no attachment-family propagation without an explicit scope decision.

## Dependencies and release gates

Matterline owns the search and review implementation. Switchbard owns this project record and its task sequencing. Release requires code, migration/API compatibility, integration coverage, browser evidence, exact deployed revision, and a production golden-case check.
