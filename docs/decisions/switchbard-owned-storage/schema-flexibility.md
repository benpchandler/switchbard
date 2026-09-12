# Schema flexibility requirement

Owner clarification, 2026-09-08: the central database schema must remain highly flexible, as today's repository files are. This is a governing requirement for the contract revision, not an optional future extension. It supplements the second opinion: structured records must not become a closed list of today's Rust fields.

## Current behavior to preserve

`crates/switchbard-core/src/backlog/write.rs` documents and implements surgical changes: editing a known field or section preserves unknown frontmatter keys, key order, quoting, formatting, and custom Markdown sections. Its `replacing_a_section_preserves_an_unknown_section_byte_for_byte` test demonstrates the custom-section case. Today's typed projections do not define all the information a task may contain. This evidence concerns task documents; it does not establish equivalent unknown-field behavior for every current YAML aggregate.

## Required design

Use a small stable storage envelope for identity, repository scope, revision, kind, lifecycle, and format/version metadata, with extensible document content. Adding a custom field, nested value, or custom body section must not require an SQL schema migration, a column per field, or a new application release merely to preserve and round-trip it. Repositories can carry different custom fields without changing the shared database schema.

Typed domain models and query indexes are projections over the complete stored content. They must never replace that content with a serialization of only the fields the current binary recognizes. Known commands patch only the content they own and update their projections transactionally. Unknown content survives edit, import/export, reopen, backup/restore, and projection rebuild. Custom values retain their types and structure; missing, null, empty string, empty list, and empty map remain distinct. Unsupported legacy YAML constructs must remain losslessly recoverable and must never be silently coerced to JSON values.

Preserve original source bytes as migration provenance and preserve untouched custom task content during ordinary edits. A readable exchange representation must retain the complete live custom content, not merely stash it in a migration backup. Byte-exact legacy source retention and readable current-state exchange are separate concerns; any conversion must define and prove the preservation rule for each record kind.

Storage must accommodate additional record kinds without a table-per-kind migration. Within a supported storage/exchange envelope, an older client preserves an unknown kind or newer payload version as opaque data and includes it unchanged in round-trip exchange. It may edit other understood records but cannot reinterpret, mutate, or silently delete the unsupported record. New domain behavior may still require code. A client that cannot honor the database's required write capabilities opens read-only or refuses writes explicitly; payload extensibility is not permission to ignore incompatible storage migrations.

Validate the stable envelope and the semantics a command owns. Arbitrary custom keys inside the designated extensible content are allowed; malformed identities, cross-repository required links, stale revisions, and unsupported required protocol semantics still reject atomically. Keep resource bounds. Separate database schema version, exchange envelope version, and per-kind content version so a new custom field does not force all three to advance. Define extension key ownership and promotion rules before implementation; a future built-in field must not silently seize or overwrite an existing custom field with the same name.

## Required acceptance scenarios

These extend MUST-004 (preservation and extensibility) and MUST-017 (exchange fidelity). They require executed product checks before acceptance; this document is not passing evidence.

1. Import a task with a custom nested map, list, null, empty values, Unicode, and a custom Markdown section. Edit its title/status through the shared core command layer. Reopen and export/import into a second database; verify all untouched content and value distinctions survive.
2. Add different custom fields to records in two repositories through the supported document-content write seam. Confirm database schema/version is unchanged and both sets remain accessible after rebuilding typed projections. Specify that seam before implementation; preservation alone is not a complete custom-content authoring contract.
3. Pass a newer payload and an unknown record kind through a compatible older client. Edit a separate understood task and export. Verify opaque records and fields survive unchanged and unsupported mutations fail without durable effects.
4. Introduce a built-in field whose name collides with existing custom data. Require an explicit, lossless promotion/mapping result; no silent overwrite or reinterpretation.
5. Exercise aggregate goal/ranking conversion with unknown fields, comments, ordering, and independent record edits. Define per-kind ownership and compare both live custom content and retained original source bytes; an archive-only copy does not satisfy live round-trip preservation.
6. Combine a custom-field edit with a concurrent known-field edit. The selected merge granularity may require explicit resolution, but neither path may discard custom content; retries and rejected imports preserve both versions.

## Revision status

The prior closed v1 schema, base64 representation, model, and verifier are not yet a completed design for this requirement. Revise them alongside exchange ancestry and record granularity before declaring implementation readiness. A strict outer protocol remains compatible with flexible inner content, but unknown kind/version handling must be specified explicitly. This amendment locks the owner's flexibility requirement and acceptance scenarios without claiming those outstanding revisions are complete.
