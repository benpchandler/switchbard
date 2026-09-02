-- sbt configuration.
-- Copy to ~/.switchbard/tui.lua and edit. sbt reloads it the moment you save.
-- Every key is optional; anything you leave out falls back to this file.
return {
  -- key -> action. Keys: single chars, "enter", "esc", "tab", "up", "down",
  -- "ctrl-<char>". Actions: down, up, top, bottom, page_down, page_up, open,
  -- back, filter, filter_column, sort_column, columns, view, command, reload, help, quit.
  keys = {
    j = "down", k = "up", down = "down", up = "up",
    g = "top", G = "bottom",
    ["ctrl-d"] = "page_down", ["ctrl-u"] = "page_up",
    enter = "open", esc = "back",
    ["/"] = "filter", f = "filter_column", s = "sort_column", [":"] = "command",
    c = "columns", v = "view", r = "reload", ["?"] = "help", q = "quit",
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

  -- Columns are picked and ordered inside sbt (`c`) and saved with each view,
  -- together with the filter and sort, in ~/.switchbard/views.lua (global) and
  -- ~/.switchbard/views/<repo>.lua (per repo). Slot 1 opens by default.
}
