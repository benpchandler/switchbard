# Formatting that helps you scan

SBT separates navigation, view context, headings and task content. The active tab is bold and underlined; attention counts keep their own badge. The list title line carries only the repo chip, the number of tasks shown and the view name (TASK-235); the editable filter renders on its own full-width line inside the list frame, below the title and above the header row, wrapping when it outgrows the width, and is simply absent when there is no filter. `/` still opens it for editing. Every other view setting — sort, shown columns, glyph mode, hidden statuses, pin, row layout, paint rule count, outline and its initiatives, live work count — moved off the title line into the bottom hint bar, muted and after the key hints, truncated from the right with an ellipsis before the key hints ever lose room. Numbered column headings split the key from the name (TASK-234): the number renders in the `keys` surface (the same ink as other key hints), the name in `header` and still bold, with an underline on the active sort spanning the whole cell. Open high-priority titles use `strong`, while their priority cells use `alert`; low-priority cells use `quiet`. Completed titles use `quiet` and their priority cells stay quiet, regardless of their former priority. Labels and glyphs still communicate the underlying facts.

## Paint from where you are

Press `p` from Tasks or Pull Requests:

- A numbered column paints its values. `auto` assigns palette slots.
- `r` paints the selected row; `f` paints the active filter.
- `e` lists values on the selected row. Selecting a value formats matching values wherever they appear. As before, the first categorical rule is the whole-row base; later categorical rules affect their own column.
- `g` offers the selected task's current group headings, including the top section. Ordinary task navigation still skips group headings.
- `h` paints column headings; `t` paints navigation and the list title. The filter and attention badges keep their own surfaces.
- `c` paints a whole column; `o` manages rule order; `d` clears all paint.

The style picker offers `quiet`, `strong`, `alert`, `band`, and `struck`, then the theme's highlight slots as swatches drawn in their own fill and default ink, then colors and palette slots. Type a combination such as `strong+p2`, `h2+alert` or `band+red` and press Enter. The typed picker title previews the composition. Esc cancels; Left returns to the prior picker. Color numbers and unique color-prefix shortcuts still work; the swatches sit between the roles and the colors, so the colors are numbered after them.

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

All colored presets declare and paint their background, so the rendered contrast checks use the same canvas the app draws. `:theme light` is available for a bright canvas; `:palette light` deliberately pairs categorical paint with it. Literal colors and independent palettes are user choices and are outside the preset contrast guarantee. `plain` retains terminal-owned colors and has no numeric contrast claim.

Customize roles with `theme.emphasis`, fills with `theme.highlights`, and surfaces with `navigation_active`, `context`, `header`, and the existing theme keys in `~/.switchbard/tui.lua`. Every colored preset declares `h1`, `h2` and `h3`; `plain` uses reverse video and terminal colors. A slot a theme leaves out is derived from the matching palette color, moved to a fixed lightness step from the canvas so the body ink still reads on it. Any role may take a background as well as a foreground, `alert = { fg = "#8a1c24", bg = "#f6c8d4" }` among them; a fill that no ink in the theme can be read on is dropped with a warning rather than rendered. Explicit `false` removes a modifier. `:theme` previews a complete preset without user surface overrides; reloading reapplies the configured theme and overrides. Set the theme name in Lua to retain it across restarts.

Working rows remain lit throughout their pulse. Set `work = { period_ms = 0 }` for steady indication. The legibility tests check actual rendered text/background pairs: every role ink on every preset fill, including highlighted cells under the cursor and through a working row's pulse, and they validate the APCA calculation against published reference pairs. Palette hues are chosen independently of the preset, so a palette token as ink carries no preset contrast claim on a fill or off it. These are project design checks, not a general accessibility certification or a guarantee about user-supplied colors.

Continuous gradients remain deferred. Existing date buckets and ordinary conditional rules remain available.
