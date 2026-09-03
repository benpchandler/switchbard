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
    accent = "cyan",
    header = "yellow",
    dim = "gray",
    selected = "#303030",
    border = "darkgray",
  },

  -- What painting a column "auto" hands out, first value first. Hex so it reads
  -- the same on every terminal palette; picked for separation on a dark background.
  palette = {
    "#ffd166", -- amber
    "#4fc3f7", -- sky
    "#7ee787", -- green
    "#ff7b72", -- coral
    "#c792ea", -- violet
    "#5ee6d8", -- teal
    "#ffa657", -- orange
    "#f78da7", -- pink
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
