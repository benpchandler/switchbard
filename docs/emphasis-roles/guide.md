# Formatting that helps you scan

SBT separates navigation, view context, headings and task content. The active tab is bold and underlined; attention counts keep their own badge. The list title line carries only the repo chip, the number of tasks shown and the view name (TASK-235); the editable filter renders on its own full-width line inside the list frame, below the title and above the header row, wrapping when it outgrows the width, and is simply absent when there is no filter. `/` still opens it for editing. Every other view setting — sort, shown columns, glyph mode, hidden statuses, pin, row layout, paint rule count, outline and its initiatives, live work count — moved off the title line into the bottom hint bar, muted and after the key hints, truncated from the right with an ellipsis before the key hints ever lose room. Numbered column headings split the key from the name (TASK-234): the number renders in the `keys` surface (the same ink as other key hints), the name in `header` and still bold, with an underline on the active sort spanning the whole cell. Open high-priority titles use `strong`, while their priority cells use `alert`; low-priority cells use `quiet`. Completed titles use `quiet` and their priority cells stay quiet, regardless of their former priority. Labels and glyphs still communicate the underlying facts.

## Paint from where you are

Press `p` from Tasks or Pull Requests:

- A numbered column paints its values. `auto` assigns palette slots.
- `r` paints the selected row; `f` paints the active filter.
- `e` lists values on the selected row. Selecting a value formats matching values wherever they appear. As before, the first categorical rule is the whole-row base; later categorical rules affect their own column.
- `g` offers the selected task's current outline headings, including the top section. Ordinary task navigation still skips outline headings.
- `h` paints column headings; `t` paints navigation and the list title. The filter, the footer's view-settings summary and attention badges keep their own surfaces — `t`'s title paint covers the title line only (repo chip, shown count, view name), not the settings that moved to the footer hint bar (TASK-235): those are facts about the view, not part of the title, and stay on `Surface::Hint`.
- `c` paints a whole column; `o` manages rule order; `d` clears all paint.
- Whatever the scope, the highlight and the text are chosen in that order, and the rule they compose is the same text `:paint` takes.

The style picker then asks two questions. **Highlight** comes first: `none`, the neutral `band`, or one of the theme's highlight slots, each drawn as a swatch in the fill it would apply. **Text** comes second: `keep default ink`, then `quiet`, `strong`, `alert`, `struck`, the colors and the palette slots. Space adds a text token and leaves the picker open, so `strong` and `p2` can go on together; Enter applies what the title is previewing. Both steps preview the composed cell live, and the rows preview themselves the same way.

Reopening a scope starts from the rule it already wears, marked on both steps. Picking a text style replaces the ink that rule had; tokens gathered with Space join each other instead. A rule's trailing `!` survives a restyle, because the marker belongs to the rule rather than to the roles. The text step holds at most fifteen tokens and says so rather than dropping one.

Typing still works at either step: a whole rule such as `h2+alert`, `band+red` or `strong+p2` applies on Enter wherever you type it. Esc cancels; Left returns to the highlight with the fill you picked still marked. Choosing `none` and then `keep default ink` clears the rule on that scope. Color numbers and unique color-prefix shortcuts still work, on the text step where the colors live.

## Roles and precedence

| Role | Meaning |
| --- | --- |
| quiet | Secondary, readable ink; explicit foreground in colored presets |
| p1, p2, ... | Categorical identity from the current palette |
| strong | Bold emphasis without a new hue |
| alert | Preset-defined attention ink and bold |
| band | The neutral fill |
| h1, h2, h3, ... | The theme's named fills, each with a default ink |
| struck | Strikethrough, for completed or excluded content |

A rule composes one fill and the ink over it. The fill is the highlight slot, or the role that carries a background of its own; every other token is ink, whichever side of the fill it is written, so `band+red` and `red+band` both write red on the band. A fill's default ink appears only when nothing else supplies ink. `h2`, `h2+alert`, `band+red` and `alert+p3` are all ordinary rules.

Rules merge non-conflicting attributes. Later, more specific rules win conflicts, and that includes the fill: two rules with fills on different scopes both paint, and a cell both claim takes the lower rule's fill. A trailing `!` stops less-specific rules for each claimed cell. The selected-row style patches over paint, and the working style patches over selection; both replace the fill under them for those cells.

Use `:paint` to open the picker, or replace the entire rule list explicitly:

```text
:paint rows:pri:high=strong;rows:status:done=quiet+struck!
:paint by:status=todo:p1,inprogress:p2+strong;header=strong
:paint heading:Done=quiet+struck;title=quiet
:paint off
```

`heading:*` matches every heading. Heading rules match the underlying group value, independently of status/progress text displayed after it. `!` belongs at the end of the whole rule, including a categorical rule with several mappings. Rule lists are bounded to 256 rules and 64 KiB.

Saved views retain role tokens, so changing presets restyles them. Existing named colors, hex values and palette tokens remain literal or palette-backed as before. Invalid saved paint is reported and the source file is protected from accidental overwrite. Rules referring to removed custom fields are pruned through the existing field-removal behavior while valid rules survive.

## Themes and motion

All colored presets declare and paint their background, so the rendered contrast checks use the same canvas the app draws. `:theme light` is available for a warm paper canvas (`#ece4d3`), softened from an earlier near-white pairing the owner found too bright and too high-contrast (TASK-239): body ink now lands within roughly Lc 75-90 against the canvas rather than above 95. `:palette light` deliberately pairs categorical paint with it. Literal colors and independent palettes are user choices and are outside the preset contrast guarantee. `plain` retains terminal-owned colors and has no numeric contrast claim.

Customize roles with `theme.emphasis`, fills with `theme.highlights`, and surfaces with `navigation_active`, `context`, `header`, and the existing theme keys in `~/.switchbard/tui.lua`. Every colored preset declares `h1`, `h2` and `h3`; `plain` uses reverse video and terminal colors. A slot a theme leaves out is derived from the matching palette color, moved to a fixed lightness step from the canvas so the body ink still reads on it. Any role may take a background as well as a foreground, `alert = { fg = "#8a1c24", bg = "#f6c8d4" }` among them; a fill that no ink in the theme can be read on is dropped with a warning rather than rendered. Explicit `false` removes a modifier. `:theme` previews a complete preset without user surface overrides; reloading reapplies the configured theme and overrides. Set the theme name in Lua to retain it across restarts.

Working rows remain lit throughout their pulse. Set `work = { period_ms = 0 }` for steady indication. The pulse moves only in OKLCH lightness, never hue, between the declared `working.bg` and a brighter endpoint (TASK-241): raw channel or luminance scaling compresses a small perceptual swing into a larger-looking channel change, which is why the pre-TASK-241 pulse read as barely perceptible on `darkroom` even though it moved a full 20% in raw luminance. The declared color is always the floor the pulse never dims below (TASK-218): on a dark canvas it is the trough and the pulse brightens 20% from there (`oklch.rs`'s `WORK_LIGHTNESS_SWING_DARK`, verified the largest of the 15-25% guidance band that still clears the Lc 75 working-row floor on berg, bloomberg and darkroom); on a light canvas, already closest to the body ink, it is the peak and the pulse brightens 12% toward the trough instead (`WORK_LIGHTNESS_SWING_LIGHT`, documented smaller because `light`'s declared peak sits close enough to the sRGB gamut's white wall that a larger swing there cannot also clear TASK-237's `h7` highlight-slot floor). Either way the ink lifts toward the theme's pole at the peak, the lower-contrast end of the swing on every preset. The legibility tests check actual rendered text/background pairs: every role ink on every preset fill, including highlighted cells under the cursor and through a working row's pulse at both extremes, and they validate the APCA calculation against published reference pairs. Palette hues are chosen independently of the preset, so a palette token as ink carries no preset contrast claim on a fill or off it. These are project design checks, not a general accessibility certification or a guarantee about user-supplied colors.

Continuous gradients remain deferred. Existing date buckets and ordinary conditional rules remain available.
