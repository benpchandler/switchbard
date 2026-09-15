# Local planning and emphasis integration

This local-only installation branch combines published planning release `5ab1d047` with installed emphasis build `b7657dbc`. It does not merge or publish the emphasis pull request. Both histories are retained. Core, task CLI and GUI source trees must remain byte-equivalent to the planning release.

## Integration changes

The only textual merge conflict was the paint-picker footer test: retain the semantic navigation/footer assertion instead of a fixed menu count. Compilation then exposed the planning section's missing raw heading value after emphasis added that field. Planned and Other tasks now carry raw values separately from their displayed text/counts so heading paint targets work. No task-writing or migration implementation is changed.

## State and stress evidence

| State | Evidence |
| --- | --- |
| Planned and other work, default planning/checklist columns, criterion completion pending manual Done | `140x28.txt`; `tests/planning_emphasis.rs` |
| Narrow terminal, clipped columns, Review still visible | `60x14.txt`; same real-key test |
| Detail section exposes Planning and checklist without changing execution | `detail.txt`; same test |
| Style targets raw heading names without display counts | Same test asserts actual terminal cell modifiers on both headings |
| Paint menu survives combined controls | Same test drives the menu and checks its heading action |
| Empty/tiny terminals, long and multilingual content, light/selected/working styles | Existing emphasis hierarchy/theme/legibility suites |
| Historical Done, no criteria, descendant progress, stale edits, saved views, cancellation | Existing planning/detail/cancellation suites |
| Web zoom, touch, remote authorization | Not applicable to this local terminal integration |

The text files are actual terminal cell buffers produced by the real renderer against synthetic task files and real key events. Temporary directory names are normalized. They do not preserve colors or native terminal font appearance; cell style assertions verify the combined heading behavior. No human visual approval is claimed.

## Validation

Full `mise run ci` passed on the combined production source, including workspace tests and developer gates (`/tmp/switchbard-planning-install-integration-ci.log`). The initial attempt reproduced the missing heading field at compile time. The additional combined real-render regression passed separately (`/tmp/switchbard-planning-emphasis-render.log`); final workspace/all-target clippy and formatting also passed with that test present. No product source changed after the successful full gate began. Core/task/GUI parity against accepted main `0a4c537f` was verified before closeout. Native font appearance and human visual approval remain explicit gaps.
