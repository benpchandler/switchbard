# Phased central-storage contract

This amendment governs implementation where older package text differs. The owner authorized gradual implementation and migration after validation, beginning with less important Markdown records and continuing through all native task-domain data. A slice is progress, not TASK-147 completion. The root migration ledger records exact migrated scopes, evidence, intentional changes and remaining work.

## Authority and sequence

The registry owns authority per `(RepositoryId, kind)`, with explicit legacy, preparing and central states. Preparing still reads and writes through the existing legacy authority; successful apply atomically imports the selected kind and activates its central authority. A process must read registry state successfully before selecting either adapter. Database failure never licenses fallback. Other kinds remain legacy until their own validated cutover.

Sequence: initiative definitions, project definitions, task config/rank/goals, then tasks. Test completed/archive/draft task fixtures before active tasks; cut over the entire task kind together because lifecycle transitions cannot straddle file and database authorities. Each slice demonstrates the same behavior or an explicitly documented intentional change through existing core commands and affected consumers before actual migration. Production migration is authorized after these checks; this contract adds no new owner approval step. Preserve legacy originals and divergence manifests, and avoid deleting unrelated records or restarting shared processes.

During mixed mode, reads compose the registered adapters and relationships preserve stable resolved targets or explicit unresolved legacy locators. No command may claim atomic success across SQLite and legacy files. A command whose related effects cross adapters must either migrate the affected set together under an established supported migration or reject before any effect with the exact unsupported mixed-mode operation. Per-slice migration tests cover such boundary operations. Ordinary migrated-kind writes update central data only; intentional writes to still-legacy kinds can continue to dirty their files and are not misreported as full no-PR completion.

## Flexible representation

Use one generic document-record store with stable identity, repository scope, kind string, revision, lifecycle and payload format/content version. Content is complete raw bytes; typed fields and indexes are rebuildable projections, never replacement serialization. Database schema version, exchange envelope version and content version are separate. Adding custom keys, nested values or kinds does not require a table/column migration merely to preserve them.

Each initiative/project/task Markdown document is one record. Initially each task-config, ranking YAML and goals YAML document is one aggregate record with domain projections for members. Whole-document revision and conflict rules deliberately cause conservative same-document conflicts; independent YAML member edits are not promised to merge automatically. Preserve unknown keys, comments, order, unsupported YAML constructs and untouched formatting in the live payload through surgical owned changes, not only in migration backups. Splitting aggregate members requires a later explicit lossless ownership mapping; it is not a prerequisite for these slices.

Provide a versioned document-content write operation taking RecordId, expected revision, command ID, full proposed raw content and format/content version. It preserves arbitrary custom structure without adding schema, validates envelope and known owned semantics, and updates indexes transactionally. Known commands patch only their existing fields/sections. Unsupported kind/content versions round-trip as opaque bytes and cannot be edited by an older client; unknown required write capabilities produce read-only/refusal. Custom field names remain custom until an explicit lossless promotion mapping; future built-ins cannot seize them silently.

## Exchange boundary

The eventual optional file contains one current state rather than duplicated complete base/current payloads. UTF-8 document payloads are readable text with exact decoded text preservation; non-UTF-8 payloads use an explicit lossless byte encoding. Resource bounds, duplicate-key rejection, stable envelope validation and scoped identities remain. Unknown compatible kinds/content remain opaque and survive exchange. Unknown required envelope capabilities still reject.

Root selected per-record version vectors in exchange-current.md. Dominance preserves newer records, stale inputs cannot revert, concurrent different content requires explicit joined-clock resolution, and epoch changes make old files reconciliation-only. Independent copies/restores rotate replica identity. Exact wire syntax, new vectors and real peer journeys still need implementation; none blocks a verified initiative-only slice. Historical vector/model scripts supply no current coverage.

## Verification and conservation

For every migrated kind, prove live byte preservation, create/read/edit/reopen, expected-revision/replay behavior, failed cutover source preservation, legacy/central adapter composition, no writes to migrated legacy files, and same or explicitly changed CLI/GUI/TUI behavior. Then validate and migrate that kind on actual data using protected recoverable backups and source digests. Record the exact result in the mission migration ledger. The whole-domain acceptance contract remains RED until every required native kind, consumer, recovery path and collaboration journey is proved.

Existing privacy, stable runtime/dispatch identity, transaction, busy bound, visibility, graph and source-retention requirements remain unless this amendment explicitly replaces their representation or sequence. Historical evidence is inventoried in historical-evidence.md and cannot satisfy current product acceptance.
