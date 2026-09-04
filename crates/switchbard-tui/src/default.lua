-- sbt configuration.
-- Copy to ~/.switchbard/tui.lua and edit. sbt reloads it the moment you save.
-- Every key is optional; anything you leave out falls back to this file.
return {
  -- key -> action. Keys: single chars, "enter", "esc", "tab", "up", "down",
  -- "ctrl-<char>". Actions: down, up, top, bottom, page_down, page_up, open,
  -- back, filter, filter_column, sort_column, columns, paint, ball, view, command, reload, help, quit.
  keys = {
    j = "down", k = "up", down = "down", up = "up",
    g = "top", G = "bottom",
    ["ctrl-d"] = "page_down", ["ctrl-u"] = "page_up",
    enter = "open", esc = "back",
    ["/"] = "filter", f = "filter_column", s = "sort_column", [":"] = "command",
    c = "columns", p = "paint", b = "ball", o = "group", t = "rank", v = "view", [","] = "settings", r = "reload", ["?"] = "help", q = "quit",
  },

  -- Colors: ANSI names (cyan, gray, darkgray, ...) follow your terminal palette;
  -- hex ("#303030") is exact. dim is secondary text (detail meta line, hints,
  -- counts); keep it readable on your background.
  -- Surfaces: each named area of the screen and how it is shaded. A bare string
  -- is a foreground color; a table sets fg, bg, bold, underline, italic, dim,
  -- reverse. Colors: ANSI names follow your terminal palette, hex is exact.
  -- `theme = "<name>"` picks a preset from `themes`; `theme = { ... }` overlays
  -- surfaces on the current preset (put both in your file to pick and tweak).
  theme = "berg",
  themes = {
    -- Berg (github.com/jx22/berg): the Bloomberg terminal as a reading theme.
    -- Body text is orange, headings and links blue, chrome gray, the cursor
    -- cyan; green and red are reserved for meaning. No background bands:
    -- only the cursor row and chips fill, so it holds on any dark background.
    berg = {
      title_repo = { fg = "black", bg = "#f49f31", bold = true },
      title      = { fg = "#acacae" },
      border     = { fg = "#464646" },
      header     = { fg = "#9e9e9e" },
      heading    = { fg = "#569cd6", bold = true },
      selected   = { bg = "#353f40", bold = true },  -- no fg: paint stays readable on the cursor row
      label      = { fg = "#acacae" },
      text       = { fg = "#f49f31" },
      link       = { fg = "#75beff" },
      chip       = { fg = "black", bg = "#f49f31" },  -- amber fill = "you can change this" on the Terminal
      keys       = { fg = "#4dc7f9" },
      hint       = { fg = "#9e9e9e" },
      status     = { fg = "#d7d7d7" },
      accent     = { fg = "#4dc7f9" },
      columns    = { id = "label", project = "link" },
    },
    -- Bloomberg data screen: amber identity chip and labels over white data.
    bloomberg = {
      title_repo = { fg = "black", bg = "#ffcc00", bold = true },
      title      = { fg = "#8b949e" },
      border     = { fg = "#30363d" },
      header     = { fg = "#8b949e" },
      heading    = { fg = "#e6edf3", bold = true, underline = true },
      selected   = { bg = "#163a63", bold = true },
      label      = { fg = "#ffcc00" },
      text       = {},
      link       = { fg = "#58a6ff" },
      chip       = { fg = "black", bg = "#ffcc00" },
      keys       = { fg = "#ffcc00" },
      hint       = { fg = "#8b949e" },
      status     = { fg = "#ffcc00" },
      accent     = { fg = "#ffcc00" },
      columns    = { id = "label", project = "link" },
    },
    -- Your terminal's own colors, nothing forced.
    plain = {
      title_repo = { bold = true },
      header     = { fg = "gray" },
      heading    = { bold = true, underline = true },
      selected   = { reverse = true },
      hint       = { fg = "gray" },
      chip       = { reverse = true },
      keys       = { bold = true },
    },
  },

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
    muted    = { "#c9b07a", "#7fa6bd", "#8db58d", "#c08a84", "#a692bd", "#7fb5ae", "#c49c7a", "#b98da0" },
    balanced = { "#ffd166", "#4fc3f7", "#7ee787", "#ff7b72", "#c792ea", "#5ee6d8", "#ffa657", "#f78da7" },
    vivid    = { "#ffcc00", "#00bfff", "#33ff66", "#ff4d4d", "#c060ff", "#00e5cc", "#ff8800", "#ff66b3" },
  },

  -- Glyphs shown when a column is in glyph mode (`c`, then `g` on the column).
  -- Keys are the column's values; a value without a glyph shows its first letter.
  glyphs = {
    priority = { high = "↑", medium = "·", low = "↓" },
    status = { icebox = "❄", todo = "○", inprogress = "◐", inreview = "◑", done = "●" },
    ball = { me = "●", agent = "◌" },
  },

  -- Columns are picked and ordered inside sbt (`c`) and saved with each view,
  -- together with the filter and sort, in ~/.switchbard/views.lua (global) and
  -- ~/.switchbard/views/<repo>.lua (per repo). Slot 1 opens by default.
}
