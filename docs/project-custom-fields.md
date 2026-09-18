# Custom project fields

Owner request, September 18, 2026: projects must support the same customization as tasks. TASK-320 owns this implementation. Custom attributes describe a project independently of its lifecycle, such as a lender's underwriting stage. Existing task fields remain task-scoped.

## Contract

Reuse the existing text, enum, date and person field types and their validation. Project declarations live under `project_fields`, separate from task `fields`, in the repository configuration authority. Project values use the native hierarchy write path and must survive legacy-file and central-database storage, rename, reload and unrelated edits. No direct database writes, parallel schema authority or GUI feature is introduced.

The same custom name may exist at task and project scope without collision. Task-only reserved names can also be project attributes, such as `project.priority`; the project's own built-in keys remain reserved. A task list projects a project value using `project.<name>`; changing that value changes the shared project, never a task override. Schema removal or enum narrowing must not strand values on any existing project, including completed or canceled ones. Invalid or stale writes fail without data loss.

## CLI usage

```sh
sb project field add lender_stage --kind enum --values 'Exploring,Underwriting,Approved,Funded' --groupable
sb project edit Huntington --set lender_stage=Underwriting
sb project view Huntington
sb project list --where lender_stage=Underwriting
sb project edit Huntington --unset lender_stage
sb project field list
```

Declarations are repository-scoped; the example does not create built-in lender terminology or seed user data. Use `sb project field edit` to change enum options or groupability and `sb project field remove` only after clearing its values. Existing `sb field` commands continue to address task declarations.

## TUI usage

Select a task linked to a defined project, then press `t f` to edit its shared project fields. The `project_fields` Lua action can also be bound directly. Select a field and enter a value; enum fields offer the declared choices and a Clear value option. Empty text clears other field kinds. Esc cancels without saving. The editor names the project and makes clear that the value is shared by all its tasks. A concurrent project edit refuses the stale save; reopen the editor after refresh.

Project fields appear as `project.<name>` in the existing column, filter, sort and grouping controls and saved views. For example, filter with `project.lender_stage:Underwriting`. Grouping requires the declaration's `--groupable` option. The task detail pane also shows the project's current attributes. A task field called `lender_stage` remains independent of `project.lender_stage`.

Define schemas through the CLI. Projects referenced only by task names need a definition created with `sb project create` before they can hold custom values. No fields or lender-specific values are automatically added to existing repositories.

## Implementation ownership

Core worker owns declaration/value persistence, validation and native stale-edit protection. CLI worker owns declaration and value commands and their integration tests. TUI worker owns projected columns, filters/sorts/groups and discoverable editing through existing surfaces, with rendered E2E tests. Root owns integration, documentation and final checks. All work uses `feat/tui-custom-project-fields`, based on origin/main, in its isolated worktree. No live installation, upstream push or merge is implied by local completion.

## State and stress matrix

| State | Expected behavior and evidence |
|---|---|
| No declarations / unset value | Existing views unchanged; blank project values remain distinguishable from task values. CLI and TUI E2E. |
| Set/edit/clear | All existing field kinds use native validation; every task in the project shows the same value after refresh. Core, CLI and TUI E2E. |
| Namespace collision | Same task/project name remains independently addressable. Core/CLI and projected-column E2E. |
| Invalid value / undeclared field | Readable error; old value and unrelated fields preserved. Native tests and TUI error state. |
| Concurrent/stale edit | Refuse stale save and preserve newer project content. Core and TUI E2E. |
| Schema narrowing/removal | Refuse while existing values would be invalid or lost, including historical project states. Core/CLI tests. |
| Central/legacy storage | Same logical result; migrated operations do not mutate retained files. Core integration tests. |
| No project / implicit project | Explicit unavailable behavior or safe native creation; never writes project fields onto the task. TUI E2E. |
| Empty/one/many/long values | Bounded rendering, truncation/wrapping and ordering follow existing list contracts. TUI E2E at narrow/current/wide widths. |
| Keyboard navigation / cancel / reload | Discoverable control; cancellation changes nothing; saved values survive reload. TUI E2E. |
| Saving/error/read-only | Existing mutation guard and clear result; no silent partial task mutation. TUI E2E. |
| External permissions/network | No new network or role system; local persistence failures use existing errors. |

## Validation evidence

Core field regression tests cover native validation, task/project namespace independence, stale writes, completed/canceled schema holders and central cutover. CLI integration tests exercise schema/value commands, filters, rename and both storage modes through the real binary. TUI tests use real key events and on-disk repositories for shared edits, cancellation, sorting/grouping/filtering, clearing, stale refusal, invalid date/person input, missing definitions and central storage. Terminal frames were inspected at 40 by 10 and 100 by 20 cells; long content follows the existing clipping behavior.

Review also reproduced a pre-existing schema writer defect through the CLI: malformed declarations could disappear and commented block headers could duplicate entries during an edit. Schema writes now reject lossy input unchanged and validate the rewritten declarations before persistence; regression tests cover both cases.

Checks passed: workspace all-targets compilation; formatting; core, CLI and TUI all-targets clippy; 34 core field tests; 21 hierarchy regression tests; four CLI project-field tests and 29 existing CLI tests; full TUI suite (577 passed, 23 ignored). The final focused TUI run passed all eight project-field tests, including the subsequently added `project.priority` independence case. The full suite caught a help-layout regression, which was fixed and passed both the affected 12-test suite and the full rerun.

Explicit limits: no live installation or owner acceptance is claimed. The guarded installer dry run refused because this branch would drop 20 commits from the installed `feat/tui-experiments` build; neither installed binary was changed. File-permission failures, maximum field counts and wider terminal sizes were not separately injected for this slice; those rows describe the intended existing contracts, not additional test evidence.
