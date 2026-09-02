-- sbt configuration.
-- Copy to ~/.switchbard/tui.lua and edit. sbt reloads it the moment you save.
-- Every key is optional; anything you leave out falls back to this file.
return {
  -- key -> action. Keys: single chars, "enter", "esc", "tab", "up", "down",
  -- "ctrl-<char>". Actions: down, up, top, bottom, page_down, page_up, open,
  -- back, filter, filter_column, sort_column, view, command, reload, help, quit.
  keys = {
    j = "down", k = "up", down = "down", up = "up",
    g = "top", G = "bottom",
    ["ctrl-d"] = "page_down", ["ctrl-u"] = "page_up",
    enter = "open", esc = "back",
    ["/"] = "filter", f = "filter_column", s = "sort_column", [":"] = "command",
    v = "view", r = "reload", ["?"] = "help", q = "quit",
  },

  -- Colors: names (cyan, darkgray, ...) or hex ("#303030").
  theme = {
    accent = "cyan",
    header = "yellow",
    dim = "darkgray",
    selected = "#303030",
    border = "darkgray",
  },

  -- Columns in the task table: id, status, priority, title, labels, project.
  columns = { "id", "status", "priority", "title" },

  -- Saved views (filter + sort in numbered slots) live in ~/.switchbard/views.lua,
  -- written by `v s <n>` inside sbt. Slot 1 opens by default.
}
