# sbt formatting, legibility and visual ergonomics

Reference for anyone changing how `sbt` colors, weights, or highlights text, and the
design basis for rule-based ("conditional") formatting. Two halves: what the research
says (sections 1-4), then what sbt has today and how it should grow (sections 5-7).
Sources are at the end.

Related backlog: TASK-152 (paint_auto stores a palette token), TASK-175/176/177
(header, tab bar, second-row differentiation), TASK-200 (paint any cell, inherit its
hierarchy). Related docs: `docs/tui-abstraction-boundaries.md`,
`docs/tui-date-paint-evidence.md`.

## 1. The formatting vocabulary a terminal actually gives you

Reliability tiers, not a flat toolkit:

| Tier | Attributes | Notes |
|---|---|---|
| Reliable | fg color, bg color, bold, underline, reverse, strikethrough | Safe to carry meaning. |
| Unreliable | dim, italic, colored/curly underline | Decorative only. Dim is often 50% alpha and lands below legible. Italic needs font plus terminal plus tmux plumbing. Terminal.app has no colored underline. |
| Banned | blink | Disabled by most terminals; an attention hazard where it works. |

Two traps:

- **Bold is not a weight.** Many terminals render bold as "bright variant of the ANSI
  color" and some render it as weight only. Bold on a hex fg is stable; bold on an ANSI
  name changes hue on some terminals. sbt already prefers hex in presets. Keep it.
- **tmux and 24-bit color.** tmux passes true color only with `terminal-overrides`. A
  user inside tmux may see hex quantized to 256 colors. Presets should still read when
  quantized (they do: the Berg and Darkroom hues are far apart in the 256 cube).

Reverse video and bg fill are the same channel: an instant, unmissable band. Budget them.

## 2. Attention hierarchy: how many "loud" things a grid can hold

Pre-attentive channels available in a monospace grid: hue, luminance, weight, fill
(bg/reverse), glyph. No size, no orientation, and motion only at a cost. Readers track
roughly three to four emphasis levels before a grid reads as noise.

The ladder, quiet to loud:

1. dim / italic (secondary metadata)
2. fg hue (categorical identity)
3. bold plus a brighter fg (needs a decision soon)
4. bg fill / reverse (needs you now, or the cursor)

Rules that follow:

- **One bg tier per screen.** sbt today spends it on `selected` and `working`. Any new
  "alert" fill has to displace or share with those, never stack a third.
- **Hue is identity, luminance is severity.** A rule that means "more urgent" should
  step luminance and weight, not switch hue. That survives grayscale, colorblindness,
  and a quantized terminal.
- **Red and green mean valence only.** Never categorical identity. Every red/green use
  carries a redundant glyph or label. sbt's palettes already put them last; keep that,
  and never let `auto` reach them when fewer than eight values exist.
- **Six to eight categorical hues per column, hard cap.** Beyond that, differentiate by
  glyph, grouping, or position.

## 3. Calls to action: making the next move obvious

What lazygit, gitui, k9s, btop and the Bloomberg terminal converge on:

- **The hint bar is content, not chrome.** It shows only the keys valid right now and
  changes as selection and mode change. A footer that reads the same every frame is
  filtered out within minutes (banner blindness, Nielsen Norman Group). sbt's footer is
  already contextual; the risk is future additions that pad it into a static list.
- **One CTA color.** Bloomberg amber means "you can change this." sbt's `chip` and
  `attention_badge` already use the amber fill for exactly that. Do not spend the same
  fill on anything that is not actionable.
- **Row-local CTA.** The eye lands on the selected row and the title before the footer.
  When an action is row-specific (merge, claim, pass), the one key that applies should
  appear on the row or the detail pane header, not only in the footer.
- **New affordance, one pop.** When a key becomes newly available (a PR turns mergeable),
  a single-frame accent on the chip is enough. Never a sustained blink.

## 4. Visual ergonomics for dark mode and long sessions

- **Contrast metric.** WCAG 2 ratios are polarity-blind and break at low luminance. Use
  APCA Lc (WCAG 3 draft). Targets against the theme background:

  | Text role | Lc target | sbt surfaces |
  |---|---|---|
  | Body | 75-90 | `text`, `label`, `link` |
  | Secondary | 45-60, never below 45 | `hint`, `header`, `title` |
  | Chrome | 15-40 | `border` |
  | Large or bold only | 60 floor | `heading`, `title_repo` |

  Body text above roughly Lc 95 on dark backgrounds causes halation (glow that smears
  into neighboring dark cells), worst on OLED and for the half of adults with some
  astigmatism. Darkroom's "body at 10:1, nothing at 14:1" rationale is this finding.
- **Dark mode is a tradeoff, not an upgrade.** Piepenbrock et al. 2013: proofreading was
  about 26% faster with fewer errors on dark-on-light. Dilated pupils on dark screens
  reduce acuity. Dark mode wins in dim rooms, on OLED, and for photophobia. A true light
  preset is a real feature for eight-hour reading, not a courtesy.
- **Near-black, not black.** Backgrounds in #121212-#1a1a1a. sbt does not paint a
  background (it inherits the terminal's), so this is guidance for the WezTerm/kitty
  scheme a preset pairs with, and a reason presets must hold on any near-black.
- **Blue and red read softer than they measure at night.** Sparse S-cones and chromatic
  aberration make saturated blue small text the worst performer on dark backgrounds;
  rods barely respond to red. Darkroom trades saturation for luminance on both. Berg's
  blue headings are large and bold, which is the one place blue is fine.
- **Motion.** WCAG 2.3.1: never more than three flashes per second. A working indicator
  should pulse at 0.5-1.5 Hz, modulate lightness only by 15-25%, never hue, never full
  on/off, and be disableable. sbt's `work` band at `period_ms = 3000` is 0.33 Hz and
  fades through black at the trough, so it currently violates the "never full off"
  guidance. `period_ms = 0` is the existing disable switch.
- **Warm at night is circadian, not legibility.** Offer it; do not claim eye-strain relief.
- **Colorblind safety.** About 8% of men have red-green deficiency. Base categorical
  palettes on Okabe-Ito or Paul Tol, separate any two must-distinguish states by at least
  15 OKLCH L points, add a glyph, and check with Sim Daltonism before shipping.
- **Generate, do not pick.** Fix hue and chroma per semantic role in OKLCH and solve L
  against the target Lc for the actual background. Quiet and loud variants of one
  semantic color vary L only. Gruvbox ships hard/medium/soft because contrast depends on
  the room; a contrast level setting is a first-class feature.

## 5. What sbt has today

Two independent layers, both fg-centric:

- **Theme surfaces** (`config.rs::Surface`, Lua `theme`): sixteen named areas, each a
  full `RawStyle` (fg, bg, bold, underline, italic, dim, reverse). `theme.columns` maps
  a column to `label`, `link`, or `text`. This is the semantic-role indirection editors
  use (Vim highlight groups, Tree-sitter captures): roles here, colors in the preset.
- **Paint rules** (`paint.rs`, `paint_eval.rs`): a per-view ordered list of `by:column`,
  `rows:filter`, `column:` rules. Lowest (most specific) claiming rule wins the cell's
  **fg only**. Colors are raw tokens; `auto` hands out palette hex.

Gaps against the research:

1. Paint carries one channel (fg). It cannot say "bold", "dim", "underline", or "band",
   so users cannot build the quiet-to-loud ladder. Every rule lands on the same rung.
2. Paint bypasses the role layer. A rule stores a hex, so themes cannot restyle it and
   Darkroom cannot lower a Bloomberg-era yellow (TASK-152's failure).
3. There is no bg-tier ceiling. Nothing stops a future rule from fighting `selected`.
4. Headings, headers, and the tab bar are one surface each with no way to say "this
   group heading is Done, paint it quiet" (TASK-175/176/177, TASK-200).
5. No contrast check for the TUI. The GUI has `tests/legibility_audit.rs`; sbt presets
   are hand-verified in comments.
6. Continuous facts (age, staleness) are only expressible as discrete threshold filters.

## 6. Proposed model: styles are roles, rules bind facts to roles

Keep the three rule kinds and the hierarchy. Change what a rule produces and where the
color comes from.

### 6.1 Emphasis roles instead of raw colors

Add a fixed vocabulary of **emphasis roles**, each a full style the preset defines, on the
ladder from section 2:

```lua
theme = {
  ...existing surfaces...,
  emphasis = {
    quiet   = { dim = true },                          -- rung 1
    p1 ... p10 = <palette slot>,                       -- rung 2, categorical identity
    strong  = { bold = true },                         -- rung 3, composes with a hue
    alert   = { fg = "#f49f31", bold = true },         -- rung 3, the one "soon" hue
    band    = { bg = "#3a2f1e" },                      -- rung 4, the one rule-owned fill
    struck  = { strikethrough = true },                -- done / closed
  },
}
```

A paint rule's right-hand side becomes a role list, not a color:

```
by:status=done:quiet+struck,inprogress:p3+strong,icebox:quiet
rows:due<today=alert
rows:ball:me&status:inreview=band
column:id=quiet
```

Roles compose by patching styles in order. A verbatim hex (`done:#ffcc00`) stays legal
and stored as typed, which satisfies TASK-152 AC #2; `auto` writes `p<n>` slot tokens,
which satisfies AC #1 and #3 and deletes `recolor_from_palettes`.

### 6.2 Precedence and merge semantics, stated once

Excel's model, which users already know:

- Rules evaluate bottom (most specific) to top (base), as today.
- **Non-conflicting attributes merge** (a `quiet` column rule plus an `alert` row rule
  gives dim amber bold). **Conflicting attributes take the most specific rule**, never a
  blend.
- A per-rule `!` suffix is "stop if true": `rows:status:done=quiet!` prevents anything
  above it from adding weight to done rows.
- `band` is a **singleton tier**: the evaluator accepts at most one rule producing `band`
  per view and refuses the second with a status-line message naming the first. `selected`
  patches over `band`, `working` patches over both. This is the structural ceiling from
  section 2, enforced in `paint_eval`, not in a comment.

### 6.3 Scopes beyond cells

TASK-200 asks to paint anything you can put the cursor on. Extend the target grammar:

```
heading:<group-value>=<roles>     -- one group section heading
header=<roles>                    -- the numbered column header row
title=<roles>                     -- tab bar / title line
```

`heading:done=quiet` is the direct answer to "highlight all the headings x color". The
picker flow is the existing `p` menu with one new entry, "this cell", which reads the
cursor's scope (cell, heading, header, title) and pre-fills the target.

### 6.4 Continuous facts

One new rule kind for dates and counts, mapping a range to a lightness ramp of one hue:

```
scale:updated=p1:30d..0d
```

Evaluated as OKLCH L interpolation between the role's quiet and loud variants. This keeps
staleness off the discrete-threshold path and is the only rule kind that produces an
interpolated color; everything else stays token-based.

### 6.5 A legibility gate for sbt

Mirror the GUI audit: a test that walks every preset's surfaces and emphasis roles,
computes APCA Lc against a declared preset background (a new `theme.background` key used
only by the test and by `scale:`), and fails on body below Lc 75, secondary below Lc 45,
and any role above Lc 100. Presets keep their contrast claims, but a test owns them.

## 7. Immediate, low-cost fixes this doc argues for

- `working` pulse should trough at roughly 40% of full band, not black, and stay within
  the 15-25% lightness swing guidance. One constant in `working_style`.
- `header` and `title` differentiation (TASK-175/176): give them a luminance step from
  body and an underline on the active sort column, no new fill. Underline is free on the
  header row because it collides with nothing.
- Second-row filter chip (TASK-177): it is a CTA (`/` edits it), so it wears `chip`. If it
  reads as noise the fix is a quieter chip variant, not a different hue.
- A `light` preset, solved in OKLCH, for daytime long reads.

## Sources

Terminal capabilities: tmux true color (gist.github.com/XVilka/8346728, tmux#696);
WezTerm `bold_brightens_ansi_colors`; microsoft/terminal#5384; undercurl support
(chromium-hterm thread, gist.github.com/andersevenrud/015e61af).
Attention: Ware, *Information Visualization*; Healey, Perception in Visualization
(csc2.ncsu.edu/faculty/healey/PP); NN/g banner blindness
(nngroup.com/articles/banner-blindness-old-and-new-findings).
Color: ColorBrewer qualitative limits; Okabe-Ito; Paul Tol; davidmathlogic.com/colorblind;
arXiv 2404.03787 on categorical perception.
Conditional formatting models: Excel rule precedence and Stop If True (support.office.com,
excelguru.ca); Neovim Tree-sitter highlight-group linking (neovim discussions #24451, #27538).
Ergonomics: APCA (git.apcacontrast.com/documentation); Piepenbrock et al. 2013,
*Ergonomics* (PubMed 23654206); Purkinje effect; WCAG 2.3.1 and 2.3.3; Harvard Health on
blue light; PMC6717920 screen filtering; 20-20-20 RCT (PubMed 36473088); Evil Martians on
OKLCH; Radix Colors and Adobe Leonardo contrast generation; Solarized and Gruvbox rationales.
