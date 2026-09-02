-- sbt configuration.
-- Copy to ~/.switchbard/tui.lua and edit. sbt reloads it the moment you save.
-- Every key is optional; anything you leave out falls back to this file.
return {
  -- key -> action. Keys: single chars, "enter", "esc", "tab", "up", "down",
  -- "ctrl-<char>". Actions: down, up, top, bottom, page_down, page_up, open,
  -- back, filter, filter_column, command, reload, help, quit, view:<name>.
  keys = {
    j = "down", k = "up", down = "down", up = "up",
    g = "top", G = "bottom",
    ["ctrl-d"] = "page_down", ["ctrl-u"] = "page_up",
    enter = "open", esc = "back",
    ["/"] = "filter", f = "filter_column", [":"] = "command",
    r = "reload", ["?"] = "help", q = "quit",
    ["1"] = "view:all", ["2"] = "view:todo", ["3"] = "view:active", ["4"] = "view:tui",
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

  -- Named filters. Filter language: free words match id or title;
  -- status:x, pri:x, label:x, project:x match that field, ignoring case and
  -- spaces (status:todo matches "To Do").
  views = {
    all = "",
    todo = "status:todo",
    active = "status:inprogress",
    tui = "label:tui",
  },
  default_view = "all",
}
