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
    c = "columns", p = "paint", b = "ball", o = "group", v = "view", r = "reload", ["?"] = "help", q = "quit",
  },

  -- Colors: ANSI names (cyan, gray, darkgray, ...) follow your terminal palette;
  -- hex ("#303030") is exact. dim is secondary text (detail meta line, hints,
  -- counts); keep it readable on your background.
  theme = {
    accent = "#ffcc00",   -- Bloomberg amber
    header = "#f0883e",
    dim = "#8b949e",
    selected = "#1f2428",
    border = "#30363d",
    heading = "white",  -- group section headings (`o`); keep it off your paint colors
  },

  -- What painting a column "auto" hands out, first value first. Pick a preset by
  -- name (`:palette <name>` inside sbt previews them live), or give your own list:
  -- palette = { "#d4b872", "#7fb3c9", ... }. Hex reads the same on every terminal.
  palette = "bloomberg",
  palettes = {
    bloomberg = { "#ffcc00", "#f0883e", "#2ea043", "#f85149", "#d29922", "#58a6ff", "#8b949e", "#c9d1d9" },
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
