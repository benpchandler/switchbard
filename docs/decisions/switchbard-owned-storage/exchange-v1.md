# Historical Switchbard repository exchange v1

RETIRED PROPOSAL: this specification is historical, not a production implementation target. See phased-contract.md and historical-evidence.md.

Owner clarification (2026-09-08): [schema flexibility](schema-flexibility.md) is a governing requirement. Use a stable envelope with extensible content, preserve unknown fields and kinds, and avoid schema migrations for custom fields. It extends MUST-004 and MUST-017. Earlier wire/schema/model details require revision where inconsistent; implementation readiness remains open.

This is the proposed normative wire specification for `.switchbard/tasks.json`. `exchange-v1.schema.json` defines closed object shapes; the algorithmic rules below are additionally mandatory. `exchange_v1_vectors.py` executes fixed specification vectors, not the product parser, import path, database, or migration. Passing these vectors does not implement TASK-147.

## Envelope and limits

The root object has exactly six required keys: `version`, `repo_id`, `digest`, `base`, `base_records`, `records`. Unknown keys reject at every object depth. `version` is the integer token `1`; `1.0`, exponent forms, strings and booleans reject. `repo_id` is a lowercase UUID in canonical 8-4-4-4-12 hexadecimal form. UUIDs are opaque identities; this wire format does not impose a UUID generation version.

`records` is the complete current snapshot, including retained tombstones. `digest` is its lowercase 64-hex SHA-256 snapshot digest defined below. `base` and `base_records` are both null for a first export. Otherwise `base` is the snapshot digest of the complete `base_records` array under the same repository identity. A transmitted base proves integrity only. Import into a populated scope additionally requires its digest to match a retained locally accepted checkpoint. Bootstrap requires explicit binding to an empty scope and records a checkpoint; it cannot overwrite populated state.

The entire UTF-8 file is limited to 67,108,864 bytes, including whitespace and embedded base. The **combined** count of `records` plus `base_records` is at most 100,000. Each raw payload decodes to at most 4,194,304 bytes. Every object or array adds one nesting level; the root object is level 1 and the maximum is 32. Scalar values do not add a level. Empty arrays and empty raw payloads are permitted. Limits are inclusive; reject excess before any durable mutation, never truncate. The schema's separate array limits do not replace the combined-count rule.

Parse strict UTF-8 without BOM. Reject invalid UTF-8, trailing data, comments, JSON conflict markers, duplicate object keys at every depth, all floating/exponent numbers, NaN/Infinity, and any decoded string containing an unpaired surrogate. Duplicate-key comparison occurs after decoding escapes: `version` and `\u0076ersion` are the same key. No numeric values other than the version field exist in v1. All schema keys are ASCII. Unicode string values are preserved without normalization; composed and decomposed strings remain distinct. A valid surrogate-pair escape decodes to one scalar and is accepted.

## Records and references

Every record has exactly these eight required fields:

| Key | Meaning |
| --- | --- |
| `record_id` | Stable lowercase canonical UUID; unique within each snapshot. |
| `kind` | One of `task`, `project`, `initiative`, `goal`, `ranking`, `config`. |
| `public_id` | Null or a nonempty string of at most 256 Unicode scalar values. Unique within `(repo_id, kind)` when present, including tombstones. It is a mutable public locator, not identity. |
| `lifecycle` | Storage classification: `active`, `completed`, `draft`, `archived`, or `tombstoned`. Existing custom task status remains in the raw payload. |
| `tombstone` | Boolean, true if and only if lifecycle is `tombstoned`. Deletion retains raw bytes and identity. |
| `raw_encoding` | Exactly `base64`. |
| `raw_payload` | Original retained payload bytes encoded using canonical RFC 4648 standard base64 alphabet with required padding and no whitespace. |
| `links` | Array of the closed references below. Empty means no recorded references, not proof that none exist. |

Each current or base snapshot contains at most one `kind: "config"` record, including retained records. When present, its `public_id` is exactly `task-config`, its lifecycle is `active`, and `tombstone` is false. Preserve its original configuration YAML bytes. An absent configuration record explicitly selects built-in defaults: `configured_statuses` is empty, `task_prefix` is `TASK`, and the normal status display uses `STANDARD_STATUSES`. Configuration deletion is not a v1 wire operation; reset explicitly writes a default configuration payload. If `base_records` contains configuration, `records` must contain configuration with the same `record_id`; absence or replacement identity rejects as forbidden configuration deletion/recreation, including on bootstrap. Import omission still follows ordinary absence-preserves-local semantics, so omission cannot reset an existing local configuration. Migration reconciles candidate configuration files to one canonical record; unused original candidates remain in database backup/provenance, not additional exchange records. Product parsing must still validate YAML meaning and typed projections.

Raw bytes need not be UTF-8. Standard alphabet means `A-Z`, `a-z`, `0-9`, `+`, `/`, and terminal `=` padding. Reject URL-safe alphabet, whitespace, omitted or surplus padding, and nonzero unused pad bits. Decode then re-encode with canonical standard padded base64 and require byte-for-byte string equality. Raw task/project/initiative payloads retain the existing document bytes; goal, ranking and configuration records retain their corresponding legacy payload bytes. The product parser must validate and derive typed indices consistently from this payload; wire schema validation alone cannot establish semantic consistency.

A resolved reference has exactly `{relation, state, repo_id, record_id}` with `state: "resolved"`. An unresolved optional reference has exactly `{relation, state, repo_id, original_name}` with `state: "unresolved_optional"`; `original_name` is a nonempty string of at most 1,024 Unicode scalar values. Both reference forms require the envelope's `repo_id`. `record_id` uses the same UUID syntax. `relation` matches `[a-z][a-z0-9_]{0,63}`. The product parser must recognize the relation and enforce its target kind, requiredness, lifecycle, existence and cycle rules against the entire merged graph. ASCII syntax is not recognition or authorization. Only genuinely optional missing legacy relationships may use `unresolved_optional`; preserve their original name and scope without manufacturing a definition. Unknown domain relations require explicit handling and cannot silently become trusted graph edges.

No machine paths, execution handles, machine provenance, secrets, timestamps, local revisions, or command receipts have wire metadata fields. Original prose/raw content remains user collaboration content; do not rewrite or silently redact it. Detailed migration and path provenance stays in the database. Workspace ordering across repositories is not a per-repository `ranking` record.

## Canonical order and exact digests

Sort each snapshot's record array by `record_id` in ascending ASCII byte order. Reject duplicate identities and unsorted record arrays. Sort each record's links by their compact canonical JSON UTF-8 bytes, ascending lexicographically; reject duplicates and unsorted links. Other order with domain meaning, such as rank order, lives inside retained raw payload bytes and must not be sorted by the envelope serializer.

Compact canonical JSON uses ASCII object-key lexicographic order, no whitespace between tokens, `,` and `:` separators, lowercase `true`/`false`/`null`, and the ordinary decimal token `1`. Strings use `"` delimiters; escape quote as `\"`, backslash as `\\`, backspace/form-feed/newline/carriage-return/tab as `\b`, `\f`, `\n`, `\r`, `\t`; encode all other U+0000 through U+001F controls as lowercase `\u00xx`. Do not escape `/`. Emit all other Unicode scalar values directly as UTF-8, including U+2028/U+2029 and non-BMP values. Do not add BOM or final newline to the compact digest input. Do not normalize Unicode.

The exact snapshot digest input is the compact canonical serialization of:

```json
{"format":"switchbard.tasks","records":[],"repo_id":"10000000-0000-4000-8000-000000000001","version":1}
```

Substitute the snapshot's actual `records` and `repo_id`. `format` is a constant domain separator inside the digest input, **not** an envelope key. Compute SHA-256 of those UTF-8 bytes and render lowercase hexadecimal. Base digest uses the identical algorithm with `base_records`. Neither digest covers the other snapshot, the envelope's digest fields, pretty whitespace, or base-envelope metadata. The separate preview source digest is SHA-256 of **all exact original file bytes** including whitespace and final newline; any file-byte change invalidates preview even if semantic snapshot digests remain equal.

## Export bytes and exchange behavior

Canonical export uses the same string and key rules, with two-space indentation per container level. Each member of a nonempty object or array appears on its own line; object separators are `: `, items end in commas except the last item, and closing delimiters occupy their own line at their parent's indentation. Empty containers are `{}` and `[]` inline. Use LF line endings and exactly one final LF; no trailing spaces. This is exactly the `pretty()` algorithm in `exchange_v1_vectors.py`. Input may use other JSON whitespace or equivalent string escapes; it must still obey record/link array ordering and all integrity rules. Canonicalization changes bytes and must invalidate an existing preview.

Exports without content/base changes reuse identical bytes. After a successor export, repeating that export must not advance its base. A populated export uses the last accepted or explicitly exported snapshot as its base. Write atomically and preserve locally modified exchange files as specified in architecture.md. These filesystem/checkpoint rules are product acceptance requirements, not established by wire vectors.

## Fixed evidence

`exchange-v1-vectors.json` contains complete golden export text in each positive vector's `file_utf8`, plus expected `snapshot_sha256`, `base_sha256`, `file_sha256`, and decoded `raw_hex`. Interpret `file_utf8` as the exact Unicode string encoded as UTF-8; its embedded final LF is part of the file digest. Goldens include combining Unicode, a non-BMP character, CRLF, NUL and a non-UTF-8 raw byte. Negative vectors cover duplicate keys at root/record/link depths, escaped duplicate keys, digest mismatch, unsupported versions, unknown fields, base64 canonicality, scope, identity duplication, invalid lifecycle, float/NaN tokens, surrogate rejection, depth overflow and BOM.

Run `python3 docs/decisions/switchbard-owned-storage/exchange_v1_vectors.py`. The runner consumes fixed goldens and rewrites only `exchange-v1-vector-results.json`; it never regenerates expected digest values. Production parsing must additionally prove exact size boundaries, streaming/resource bounds, local checkpoint trust, typed graph consistency, preview transactions, and migration conservation. The separate smaller synthetic SQLite model intentionally uses its own abbreviated record representation and is not a wire implementation.
