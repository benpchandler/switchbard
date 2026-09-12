# Current-state exchange with per-record causal clocks

Root selected this reduced algorithm after independent review. It replaces the historical embedded-base proposal and the intermediate retained-snapshot candidate. This is the implementation contract for the later exchange slice, not passing product evidence. Central per-kind migration proceeds independently.

## Single current-state file

The optional file carries a version/capabilities envelope, RepositoryId, epoch_id, current records including tombstones, and a semantic snapshot digest. It carries no duplicated base snapshot and requires no known-base checkpoint to accept a causally newer record. Each record has stable identity, kind/content version, lifecycle, complete payload and a version-vector map from replica UUID to positive integer counter.

UTF-8 document content is readable tagged text with exact decoded text preservation; non-UTF-8 content uses explicitly tagged lossless bytes only where necessary. Kind remains extensible and unknown compatible content remains opaque. Unknown required protocol/write capabilities reject. Typed known semantics validate without serializing away custom content. exchange-v2.md and exchange-v2.schema.json describe the actual implemented syntax. Independent fixed vectors still must be exercised against the product parser; historical v1 schema/vectors are not normative.

## Replica and mutation rules

Each independently writing database has a stable replica UUID. Independently restored/copied databases rotate replica identity before writes so two writers cannot generate the same causal component/counter independently. Ordinary reopen of the same database keeps its identity. Empty exchange bootstrap also mints a distinct local replica identity. Raw database copying is unsupported as collaboration transport; supported restore/copy-to-new-peer rotates identity, or use exchange bootstrap. Preserve existing clocks when rotating; do not erase history or pretend replica rotation resets causal state.

A local document edit or tombstone increments the local replica component atomically with payload, revision and command receipt. A missing clock component compares as zero. Replayed commands return the prior result and cannot increment again. Config reset edits its existing record; deletion/recreation remains forbidden. Source migration initializes each new record with the importing replica's first event while preserving migration provenance.

## Import and explicit resolution

For matching RecordId, compare incoming and local clocks componentwise. Incoming strictly dominates local: take incoming. Local strictly dominates incoming: retain local. Equal clocks require identical semantic content; unequal content with equal clocks rejects as corrupt/ambiguous input. Concurrent clocks with identical content join componentwise maxima without inventing content. Concurrent different content creates an explicit conflict, including edit-versus-tombstone.

An explicit local/incoming/custom resolution joins both clocks then increments the resolver's component. Preserve all originals. Omitted incoming records are no-ops; only explicit causally versioned tombstones delete. Stable locator uniqueness remains (RepositoryId, kind, public_id), allowing matching names across kinds. Validate the whole merged candidate graph and singular config before one atomic apply. Recheck exact source digest and expected repository revision; rejection has zero live effects.

This permits first publish/bootstrap/edit/export-back, arbitrary skipped intermediate exports, and stale-file imports without rollback. No timestamp winner and no receiver-baseline ancestry guess participates. If the incoming epoch differs, require explicit reconciliation/restore; never automatically replace the active scope.

## Bounds and retention

Keep the existing 64 MiB file, 100,000 current records, 4 MiB payload and depth 32 limits. Bound each clock to 128 replica entries and each counter to the inclusive integer range 1 through 9,007,199,254,740,991 (exact JSON safe integer range). Reject zero/fractional/overflow counters, malformed replica IDs and excess entries before effects. An exhausted counter or clock limit refuses further mutation with explicit compaction guidance; never truncate entries or wrap counters. These interoperability constants require boundary product tests and independent vectors.

Retain tombstones while unknown offline peers may possess the epoch. No safe automatic garbage collection follows from inactivity. Explicit recoverable compaction starts a new epoch; old-epoch files become reconciliation-only. Clock metadata growth and this boundary must be observable. A backup copy is not a peer acknowledgment.

## Required real product journeys

First publish/empty bootstrap/edit/export-back; arbitrarily skipped exports; alternating/disjoint convergence; same-document concurrent conflict; stale snapshot no-rollback; edit/tombstone conflict; omission preservation; equal-clock unequal-content rejection; identical concurrent-content clock join; joined-clock resolution; independent copy/restore replica rotation; wrong epoch and explicit compaction; exact clock/file limits. Include readable PR diffs, flexible unknown content/kind round-trip and duplicate-key rejection. Independent wire vectors and actual product executions remain required; historical model successes cannot satisfy them.
