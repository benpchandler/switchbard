# Formatting that helps you scan

SBT separates navigation, view context, headings and task content. The active tab is bold and underlined; attention counts keep their own badge. The view context leads with the number of tasks shown, then the view and editable filter. Numbered column headings are bold, with an underline on the active sort. High-priority titles and priority cells use `strong`; completed titles use `quiet+struck`. Labels and glyphs still communicate the underlying facts.

## Paint from where you are

Press `p` from Tasks or Pull Requests:

- A numbered column paints its values. `auto` assigns palette slots.
- `r` paints the selected row; `f` paints the active filter.
- `e` lists values on the selected row. Selecting a value formats matching values wherever they appear. As before, the first categorical rule is the whole-row base; later categorical rules affect their own column.
- `g` offers the selected task's current group headings, including the top section. Ordinary task navigation still skips group headings.
- `h` paints column headings; `t` paints navigation and the list title. The filter and attention badges keep their own surfaces.
- `c` paints a whole column; `o` manages rule order; `d` clears all paint.

The style picker offers `quiet`, `strong`, `alert`, `band`, and `struck`, followed by colors and palette slots. Type a combination such as `strong+p2` and press Enter. The typed picker title previews the composition. Esc cancels; Left returns to the prior picker. Existing color numbers and unique color-prefix shortcuts still work.

## Roles and precedence

| Role | Meaning |
| --- | --- |
| quiet | Secondary, readable ink; explicit foreground in colored presets |
| p1, p2, ... | Categorical identity from the current palette |
| strong | Bold emphasis without a new hue |
| alert | Preset-defined attention ink and bold |
| band | The one rule-owned background tier |
| struck | Strikethrough, for completed or excluded content |

Rules merge non-conflicting attributes. Later, more specific rules win conflicts. A trailing `!` stops less-specific rules for each claimed cell. Exactly one rule per view may use `band`; adding another reports which rule owns it and leaves the view unchanged. The selected-row style patches over paint, and the working style patches over selection. Background or reverse-video customization belongs only to `band`, preventing custom roles from bypassing that limit.

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

Customize roles with `theme.emphasis` and surfaces with `navigation_active`, `context`, `header`, and the existing theme keys in `~/.switchbard/tui.lua`. Explicit `false` removes a modifier. `:theme` previews a complete preset without user surface overrides; reloading reapplies the configured theme and overrides. Set the theme name in Lua to retain it across restarts.

Working rows remain lit throughout their pulse. Set `work = { period_ms = 0 }` for steady indication. The legibility tests check actual rendered text/background pairs, including selected and working rows, and validate the APCA calculation against published reference pairs. These are project design checks, not a general accessibility certification or a guarantee about user-supplied colors.

Continuous gradients remain deferred. Existing date buckets and ordinary conditional rules remain available.
