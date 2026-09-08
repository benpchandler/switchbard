# Saved list view scope (TASK-166)

The abstraction stays in the TUI: both concrete consumers already share `Column`, `Sort`, `Grouping`, `PaintRule`, and `ViewState`. `ListSettings(Page)` now owns the list catalog, default columns, legacy alias interpretation, grouping/top-list/abbreviation capabilities, starter views, and the existing `.prs.lua` file suffix. Core does not acquire a presentation model. `ViewStore` and its Lua record parser/serializer remain the sole persistence authority for slots and self-restart; no registry or second store is added. TASK-153/TASK-154 startup/history semantics remain separate.

The Tasks paths remain `views.lua` and the existing repo override path; Pull Requests derives `.prs.lua` from each. The shared App load path uses that scope, as does `page_columns`. PR `status` aliases canonicalize to `lifecycle` across columns, glyphs, sorting and paint. Filed and Merged use the same catalog identities without new serialization cases.

## Compatibility and preservation policy

Older task records with omitted fields retain the established defaults. Recognized aliases load and serialize under their canonical names. Missing files use starter slots and can be saved. Unknown columns, fields, sorting/grouping forms, malformed Lua, invalid paint rules or unsupported slot numbers produce a warning and prevent writes to the affected file. Recognized columns that are unsupported on the current feature are omitted from the usable in-memory view but block replacement of their source. Unsupported feature grouping likewise blocks replacement. Existing unsupported data is not silently rewritten as defaults.

Valid portions of a malformed paint string are not used to overwrite its source; unknown target columns and partially invalid categorical color mappings are rejected as persisted records. Free-text filter grammar retains its existing behavior.

Each source also retains its startup bytes. A file changed externally after loading blocks saving or promotion until reopen. Repairing a malformed source and reopening enables writes again. A blocked global source still permits an independent repo override save; promotion checks both files before changing either. Failed repo saves leave in-memory slots unchanged. Promotion is explicitly two writes: if global succeeds and repo removal fails, the confirmed global state and source guard are retained, the repo override remains, and the UI reports `global saved; repo override retained; retry after repair`. Repairing the failed path allows retry; later external edits still block retry. This is local conflict detection, not an atomic cross-file transaction or a filesystem locking protocol.

Global slots must be contiguous from 1; sparse global Lua sequences are rejected without rewriting. Sparse repo overrides remain valid: slot 9 stays slot 9 across picker labels/keys, help, direct load, save and restart. Promotion fills preceding global slots only from actual existing views and refuses a remaining gap, without inventing defaults or compacting identities.

## State and stress matrix

| State or transition | Evidence |
| --- | --- |
| Older task defaults, known aliases, applicable filter/sort/columns/glyph/paint/group settings | `view_scope::legacy_slots_and_both_feature_records_survive_save_and_restart` |
| Tasks/PR independent slot saves and self-restart | Same test; existing `views` and `pr_controls` suites |
| Missing source, ordinary save/global promotion | Existing `views` suite |
| Malformed Lua, future fields/columns/sort forms, repeated save/promotion | `view_scope::malformed_and_future_records_are_preserved_on_save_and_promotion` |
| External edit, conflict, reopen recovery, other-page save | `view_scope::external_edit_blocks_save_and_reopen_recovers_without_changing_other_page` |
| Unsupported PR grouping/columns | `view_scope::unsupported_feature_columns_and_grouping_do_not_get_overwritten` |
| Global failure with independent repo save; failed promotion preserves both | `view_scope::malformed_global_file_allows_repo_save_but_cannot_be_promoted_over` |
| Sparse global sequences and sparse repo slot 9 | `view_scope::sparse_global_slots_are_preserved_instead_of_truncated_on_promotion`, `view_scope::sparse_repo_slot_keeps_nine_in_picker_help_load_save_and_restart` |
| Promotion second-write failure and repair retry; external edit after partial result | `view_scope::promotion_reports_confirmed_global_write_and_retries_repo_removal`, `view_scope::partial_promotion_retry_never_overwrites_later_external_global_edit` |
| Partial invalid paint records | `view_scope::partial_paint_decode_never_overwrites_original_rules` |
| Keyboard-only current 100x20 terminal | All new tests render through real App keys and TestBackend |
| Network loading/offline | Persistence is local; PR test renders controls without requiring remote data |
| Permission errors | Read failures block writes; no dedicated permission fixture because root/write permissions differ across environments |
| Narrow/wide clipping, pointer/touch, remote authority roles | Layout and input widgets unchanged; no new visual geometry or remote authority |

## Verification

`cargo test -p switchbard-tui --test view_scope` exercises ten new end-to-end tests through actual keys, real temporary files and rendered output. Existing `views` (3 passing) and ordinary `pr_controls` (4 passing, 6 explicitly live-only ignored) checks cover prior commands and feature separation. `cargo clippy -p switchbard-tui --all-targets -- -D warnings` checks the integrated TUI. Root owns installation, live sbt dogfooding, repository-wide final gates and delivery; these tests do not claim owner visual approval.


## Review repairs (TASK-191 / TASK-192)

Before fixing, the real-key tests reproduced both issues. An array-form `return { {filter='label:auth'}, nil, {filter='label:ui'}, {filter='label:docs'} }` had Lua raw length 4 but sequence enumeration stopped after slot 1; `vgd` reported success and overwrote later entries. A repo file with only `[9]` displayed that record as picker 6 and rejected `v9`. Explicit slot identities and contiguous global validation resolve both cases.

For partial promotion, create a directory at the repo source's temporary path `views-repo.lua.tmp` after an ordinary `vsd`. `vgd` writes global successfully but cannot write the repo temporary file. Before repair, the UI showed only a generic directory write error and the store retained its old global source guard. It now reports the confirmed global result, retains the repo override and retries successfully after removing the obstructing directory. The separate external-edit test proves a later global change is preserved instead of overwritten by retry.
