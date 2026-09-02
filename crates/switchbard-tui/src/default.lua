-- sbt configuration.
-- Copy to ~/.switchbard/tui.lua and edit. sbt reloads it the moment you save.
-- Every key is optional; anything you leave out falls back to this file.
return {
  -- key -> action. Keys: single chars, "enter", "esc", "tab", "up", "down",
  -- "ctrl-<char>". Actions: down, up, top, bottom, page_down, page_up, open,
  -- back, filter, command, reload, help, quit, view:<name>.
  keys = {
    j = "down", k = "up", down = "down", up = "up",
    g = "top", G = "bottom",
    ["ctrl-d"] = "page_down", ["ctrl-u"] = "page_up",
    enter = "open", esc = "back",
    ["/"] = "filter", [":"] = "command",
    r = "reload", ["?"] = "help", q = "quit",
    ["1"] = "view:active", ["2"] = "view:all", ["3"] = "view:tui",
  },

  -- Colors: names (cyan, darkgray, ...) or hex ("#303030").
  theme = {
    accent = "cyan",
    dim = "darkgray",
    selected = "#303030",
    border = "darkgray",
  },

  -- Columns in the task table: id, status, priority, title, labels, project.
  columns = { "id", "status", "priority", "title" },

  -- Named filters. Filter language: free words match id or title;
  -- status:x, pri:x, label:x, project:x match that field (substring, any case).
  views = {
    all = "",
    active = "status:progress",
    tui = "label:tui",
  },
  default_view = "all",
}
