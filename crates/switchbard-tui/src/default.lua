-- sbt configuration.
-- Copy to ~/.switchbard/tui.lua and edit. sbt reloads it the moment you save.
-- Every key is optional; anything you leave out falls back to this file.
return {
  -- key -> action. Keys: single chars, "enter", "esc", "tab", "up", "down",
  -- "pagedown", "pageup", "home", "end", "shift-tab", "ctrl-<char>". Actions: down, up, top, bottom, page_down, page_up, open, new_task,
  -- back, focus_pane, filter, filter_column, sort_column, columns, paint, ball, pass, outline, task, settings, view, command, reload, help, quit, page, merge, open_browser, dismiss_notifications.
  keys = {
    j = "down", k = "up", down = "down", up = "up",
    g = "top", G = "bottom", home = "top", ["end"] = "bottom",
    pagedown = "page_down", pageup = "page_up",
    ["ctrl-d"] = "page_down", ["ctrl-u"] = "page_up",
    enter = "open", esc = "back", tab = "page", ["shift-tab"] = "focus_pane",
    ["/"] = "filter", f = "filter_column", s = "sort_column", [":"] = "command",
    c = "columns", p = "paint", b = "ball", w = "pass", o = "outline", t = "task", v = "view", [","] = "settings", r = "reload", O = "open_browser", m = "merge", n = "dismiss_notifications", ["?"] = "help", q = "quit",
  },

  -- Colors: ANSI names (cyan, gray, darkgray, ...) follow your terminal palette;
  -- hex ("#303030") is exact. dim is secondary text (detail meta line, hints,
  -- counts); keep it readable on your background.
  -- Surfaces: each named area of the screen and how it is shaded. A bare string
  -- is a foreground color; a table sets fg, bg, bold, underline, italic, dim,
  -- reverse, strikethrough. Explicit false clears an inherited modifier. Colors: ANSI names follow your terminal palette, hex is exact.
  -- `theme = "<name>"` picks a preset from `themes`; `theme = { ... }` overlays
  -- surfaces on the current preset (put both in your file to pick and tweak).
  -- `:theme <name>` inside sbt switches presets live to try them; it is
  -- in-memory, so write the name here to keep it.
  -- theme.emphasis defines quiet, strong, alert, band and struck; theme.highlights
  -- defines the named fills h1, h2, h3 ... (bg plus the ink that reads on it).
  -- Any role may take a bg too (alert = { fg = "#8a1c24", bg = "#f6c8d4" }); a
  -- fill no ink in the theme can be read on is dropped with a warning. A slot a
  -- theme leaves out is derived from the matching palette color at a fixed
  -- distance from the background.
  -- Custom themes can omit roles: hint/accent/working supply safe defaults.
  -- Paint accepts roles, highlight slots, colors and palette slots joined with +
  -- (strong+p2, h2, h2+alert, band+red): the fill comes from the slot or the
  -- role that has one, the ink from everything else.
  -- A trailing ! makes that paint rule take precedence over competing rules.
  -- theme.background is the explicit canvas; leave absent for terminal control.
  -- pill requires Powerline rounded caps; ascii uses parentheses; icons keeps glyphs.progress.
  -- Explicit glyphs.progress overrides select icons unless progress_style is also set.
  progress_style = "pill",
  theme = "berg",
  themes = {
    -- Exact foregrounds and canvases make each colored preset predictable.
    -- Quiet ink stays readable without terminal-dependent DIM. Plain delegates
    -- contrast to the terminal. These presets are not an accessibility certification.
    berg = {
      progress_fill = { fg = "#528fff" },
      progress_complete = { fg = "#ff565f" },
      progress_empty = { fg = "#528fff" },
      progress_shell = { fg = "#263e61" },
      background = "#101214",
      title_repo     = { fg = "#101214", bg = "#f49f31", bold = true },
      title          = { fg = "#acacae" },
      navigation_active = { fg = "#85d9f7", bold = true, underline = true },
      context = { fg = "#acacae" },
      border         = { fg = "#464646" },
      header         = { fg = "#b5bbc3", bold = true },
      heading        = { fg = "#99c6ec", bold = true },
      selected       = { bg = "#242d2e" },
      label          = { fg = "#acacae" },
      text           = { fg = "#ffd190" },
      link           = { fg = "#99c6ec" },
      chip           = { fg = "#101214", bg = "#f49f31" },
      attention_badge= { fg = "#101214", bg = "#f49f31", bold = true },
      keys           = { fg = "#85d9f7" },
      hint           = { fg = "#acacae" },
      status         = { fg = "#acacae" },
      accent         = { fg = "#85d9f7" },
      working        = { bg = "#163b30", bold = true },
      emphasis = {
        quiet = { fg = "#acacae", dim = false },
        strong = { bold = true },
        alert = { fg = "#ffb8aa", bold = true },
        band = { bg = "#3b3023" },
        struck = { strikethrough = true },
      },
      highlights = {
        h1 = { bg = "#3f3020", fg = "#ffd190" },
        h2 = { bg = "#14343a", fg = "#ffd190" },
        h3 = { bg = "#3d2531", fg = "#ffd190" },
      },
      columns = { id = "label", project = "link", goal = "link" },
    },
    bloomberg = {
      progress_fill = { fg = "#528fff" },
      progress_complete = { fg = "#ff565f" },
      progress_empty = { fg = "#528fff" },
      progress_shell = { fg = "#263e61" },
      background = "#111820",
      title_repo     = { fg = "#111820", bg = "#ffcc00", bold = true },
      title          = { fg = "#a8afb8" },
      navigation_active = { fg = "#ffcc00", bold = true, underline = true },
      context = { fg = "#a8afb8" },
      border         = { fg = "#394754" },
      header         = { fg = "#b5bbc3", bold = true },
      heading        = { fg = "#e6edf3", bold = true },
      selected       = { bg = "#172d42" },
      label          = { fg = "#a8afb8" },
      text           = { fg = "#e6edf3" },
      link           = { fg = "#99c6ec" },
      chip           = { fg = "#111820", bg = "#ffcc00" },
      attention_badge= { fg = "#111820", bg = "#ffcc00", bold = true },
      keys           = { fg = "#ffcc00" },
      hint           = { fg = "#a8afb8" },
      status         = { fg = "#a8afb8" },
      accent         = { fg = "#ffcc00" },
      working        = { bg = "#173725", bold = true },
      emphasis = {
        quiet = { fg = "#a8afb8", dim = false },
        strong = { bold = true },
        alert = { fg = "#ffb8aa", bold = true },
        band = { bg = "#303b49" },
        struck = { strikethrough = true },
      },
      highlights = {
        h1 = { bg = "#3d3520", fg = "#e6edf3" },
        h2 = { bg = "#23394f", fg = "#e6edf3" },
        h3 = { bg = "#3a2a44", fg = "#e6edf3" },
      },
      columns = { id = "label", project = "link", goal = "link" },
    },
    darkroom = {
      progress_fill = { fg = "#528fff" },
      progress_complete = { fg = "#ff565f" },
      progress_empty = { fg = "#528fff" },
      progress_shell = { fg = "#263e61" },
      background = "#12100e",
      title_repo     = { fg = "#e3ddd3", bg = "#634921", bold = true },
      title          = { fg = "#b3aa9e" },
      navigation_active = { fg = "#b7d1cd", bold = true, underline = true },
      context = { fg = "#b3aa9e" },
      border         = { fg = "#51483f" },
      header         = { fg = "#c0b2a0", bold = true },
      heading        = { fg = "#e4bd7e", bold = true },
      selected       = { bg = "#302820" },
      label          = { fg = "#b3aa9e" },
      text           = { fg = "#e3ddd3" },
      link           = { fg = "#a9c7e7" },
      chip           = { fg = "#e3ddd3", bg = "#634921" },
      attention_badge= { fg = "#e3ddd3", bg = "#634921", bold = true },
      keys           = { fg = "#b7d1cd" },
      hint           = { fg = "#b3aa9e" },
      status         = { fg = "#b3aa9e" },
      accent         = { fg = "#b7d1cd" },
      working        = { bg = "#253c3c", bold = true },
      emphasis = {
        quiet = { fg = "#b3aa9e", dim = false },
        strong = { bold = true },
        alert = { fg = "#eebbb0", bold = true },
        band = { bg = "#393026" },
        struck = { strikethrough = true },
      },
      highlights = {
        h1 = { bg = "#43331e", fg = "#e3ddd3" },
        h2 = { bg = "#26351f", fg = "#e3ddd3" },
        h3 = { bg = "#22303f", fg = "#e3ddd3" },
      },
      columns = { id = "label", project = "link", goal = "link" },
    },
    -- A warm paper canvas, not near-white: TASK-239 softened this from an
    -- earlier #f5f2eb/#1f2830 pairing the owner found too bright and too
    -- high-contrast (body text landed above Lc 95). Body ink now lands
    -- within roughly Lc 75-90 against the canvas (tests/legibility.rs
    -- asserts both the floor and this ceiling).
    light = {
      progress_fill = { fg = "#245dcc" },
      progress_complete = { fg = "#b81e35" },
      progress_empty = { fg = "#245dcc" },
      progress_shell = { fg = "#c5d4eb" },
      background = "#ece4d3",
      title_repo     = { fg = "#ece4d3", bg = "#355b7b", bold = true },
      title          = { fg = "#59616b" },
      navigation_active = { fg = "#315f65", bold = true, underline = true },
      context = { fg = "#59616b" },
      border         = { fg = "#a3a9ae" },
      header         = { fg = "#3d4d5e", bold = true },
      heading        = { fg = "#334b63", bold = true },
      selected       = { bg = "#e5ddcd" },
      label          = { fg = "#59616b" },
      text           = { fg = "#3a362d" },
      link           = { fg = "#355b7b" },
      chip           = { fg = "#ece4d3", bg = "#355b7b" },
      attention_badge= { fg = "#ece4d3", bg = "#355b7b", bold = true },
      keys           = { fg = "#315f65" },
      hint           = { fg = "#59616b" },
      status         = { fg = "#59616b" },
      accent         = { fg = "#315f65" },
      working        = { bg = "#b7d7c7", bold = true },
      emphasis = {
        quiet = { fg = "#59616b", dim = false },
        strong = { bold = true },
        alert = { fg = "#8a302d", bold = true },
        band = { bg = "#ecdcae" },
        struck = { strikethrough = true },
      },
      highlights = {
        h1 = { bg = "#f1e2c4", fg = "#1f2830" },
        h2 = { bg = "#f6dcdc", fg = "#1f2830" },
        h3 = { bg = "#e4e9d6", fg = "#1f2830" },
      },
      columns = { id = "label", project = "link", goal = "link" },
    },
    -- Your terminal owns its colors and background.
    plain = {
      progress_fill = {},
      progress_complete = {},
      progress_empty = {},
      progress_shell = {},
      title_repo = { bold = true },
      navigation_active = { bold = true, underline = true },
      context = {},
      header = { bold = true },
      heading = { bold = true, underline = true },
      selected = { reverse = true },
      chip = { reverse = true },
      attention_badge = { reverse = true },
      keys = { bold = true },
      working = { bold = true, underline = true },
      emphasis = {
        quiet = { dim = true },
        strong = { bold = true },
        alert = { bold = true, underline = true },
        band = { reverse = true },
        struck = { strikethrough = true },
      },
      -- Your terminal's own palette, so these carry no measured contrast.
      highlights = {
        h1 = { reverse = true },
        h2 = { bg = "blue", fg = "white" },
        h3 = { bg = "magenta", fg = "black" },
      },
    },
  },

  -- Rows a live agent session is working (`sb work claim`) pulse: the
  -- `working` band breathes gently over period_ms, brightest at the start,
  -- with its tops and bottoms flattened by `flatten` (0 = pure sine, larger
  -- holds peak and trough longer), redrawn frames times per period. period_ms
  -- = 0 keeps them steady; the band never disappears. The `work` column (`c`) shows one ● per session; `w`
  -- passes the task.
  -- PR reads refresh while the page is visible (30-3600 seconds); failures require r.
  pr_refresh_seconds = 60,

  work = { period_ms = 3000, frames = 30, flatten = 2 },

  -- What painting a column "auto" hands out, most common value first: keep the
  -- first color quiet so the common case stays calm and the rest stand out. Pick a preset by
  -- name (`:palette <name>` inside sbt previews them live), or give your own list:
  -- palette = { "#d4b872", "#7fb3c9", ... }. Hex reads the same on every terminal.
  -- Where `:bug` and `:idea` file. sbt's own repo, so reports about the tool
  -- never land in the backlog you happen to be browsing. Unset files locally.
  -- report_repo = "~/Dev/switchbard",

  palette = "berg",
  palettes = {
    -- Berg: Bloomberg's categorical hues (the Terminal's chart legend and
    -- vim-bloomberg), not price colors. Orange first because it is the body
    -- text, so the common value stays calm; lavender, mint, magenta, gold,
    -- blue, sky next; green and red last because they read as up/down.
    berg      = { "#f49f31", "#c6c5fe", "#4af6c3", "#ff73fd", "#e0c010", "#0b85df", "#96cbfe", "#a8ff60", "#ff6c60", "#acacae" },
    bloomberg = { "#c9d1d9", "#ffcc00", "#2ea043", "#58a6ff", "#f0883e", "#f85149", "#d29922", "#8b949e" },
    light = { "#1f2830", "#355b7b", "#653459", "#315f50", "#6b4427", "#8a302d" },
    muted    = { "#c9b07a", "#7fa6bd", "#8db58d", "#c08a84", "#a692bd", "#7fb5ae", "#c49c7a", "#b98da0" },
    balanced = { "#ffd166", "#4fc3f7", "#7ee787", "#ff7b72", "#c792ea", "#5ee6d8", "#ffa657", "#f78da7" },
    vivid    = { "#ffcc00", "#00bfff", "#33ff66", "#ff4d4d", "#c060ff", "#00e5cc", "#ff8800", "#ff66b3" },
    -- Darkroom: ten hues at 7.0-8.2:1 against near-black. Not one flat number --
    -- a contrast ratio is luminance only, and blue and red both read softer than
    -- they measure at night (sparse S-cones for blue; rods, which carry night
    -- vision, barely respond to red). Those two trade saturation for luminance.
    -- Sand first -- nearest the body text, so the most common value stays quiet;
    -- green and red last, they read as up/down.
    darkroom = { "#A89787", "#C3964A", "#91A8C7", "#71AAAB", "#C695AA", "#B09FCB", "#82AABC", "#A7A04B", "#78A957", "#D0978B" },
  },

  -- Glyphs shown when a column is in glyph mode (`c`, then `g` on the column).
  -- Keys are the column's values; a value without a glyph shows its first letter.
  glyphs = {
    -- Icons mode: 0, >0-<34, 34-<67, 67-<100, exactly 100%.
    -- Pill mode has fixed compact symbols when fewer than six cells fit.
    -- No retained criteria (including canceled tasks) is unmeasured.
    progress = { empty = "○", low = "◔", medium = "◑", high = "◕", complete = "●", unmeasured = "-" },
    priority = { high = "↑", medium = "·", low = "↓" },
    status = { icebox = "❄", todo = "○", inprogress = "◐", inreview = "◑", done = "●" },
    ball = { me = "●", agent = "◌" },
    lifecycle = { open = "○", closed = "×", merged = "●" },
    checks = { failed = "!", unknown = "?", pending = "~", passed = "+", noneobserved = "?", notfetched = "·" },
    review = { changesrequested = "!", reviewunknown = "?", reviewrequired = "○", approved = "+" },
    merge = { mergeconflict = "!", mergeabilityunknown = "?", nomergeconflict = "+" },
    draft = { draft = "D", ready = "R" },
  },

  -- Columns are picked and ordered inside sbt (`c`) and saved with each view,
  -- together with the filter and sort, in ~/.switchbard/views.lua (global) and
  -- ~/.switchbard/views/<repo>.lua (per repo). Slot 1 opens by default.
  -- PR views use views.prs.lua and views/<repo>.prs.lua, independently.
  -- PR column names for theme/glyphs: id, lifecycle, tasks, checks, title, review, merge, draft.
}
