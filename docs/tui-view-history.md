# Resume and view history

`sbt` resumes your last view when you open it in the same repo, including independent Tasks and Pull Requests arrangements and the active page. It keeps the filter, sort, columns, glyphs, abbreviations, paint, grouping, top-list visibility, row layout and selection in the same record used by an automatic binary restart.

Run `sbt --fresh` (or `sbt --repo /path/to/repo --fresh`) to open your saved default instead. The first launch in a repo also opens that default. `--fresh` affects launch only; the new session still records its own arrangement. If the binary updates itself during that session, its restart retains the current view.

Open **Views > History** with `v h`. Each row describes the arrangement and when you used it. Search by typing, use arrows to select, and read the wrapped preview with PgUp/PgDn before pressing Enter to restore. After restoring, use `v s <number>` to keep it in a numbered slot. `v s d` deliberately changes the repo default; `v g d` promotes the default globally. Neither launch nor automatic capture writes a saved slot.

History is captured automatically every 30 seconds, including while a picker is open, and on quit or a handled terminal-close signal (SIGHUP, SIGTERM or SIGINT). All triggers use the same checkpoint path. A forced kill resumes the latest completed checkpoint. Returning to an existing arrangement moves it to the front instead of creating a duplicate; idle timer ticks do not fill the history. The first capture in a new session refreshes the visit time. Names belong to slots and do not create duplicate arrangements.

History keeps 30 days, at most 1000 distinct arrangements across Tasks and Pull Requests, and at most 4 MiB per repo. An individual arrangement is limited to 64 KiB. The oldest entries are removed first when a ceiling is reached. Tasks and Pull Requests have separate history lists; Inbox resumes its active page but has no arrangement history.

Automatic paint stores palette slots such as `p1`. Restoring that view uses your currently selected palette. Named colors and hex colors you deliberately select remain literal. Older saved views with hex colors still load without a migration; their literals are retained because the record cannot distinguish an old automatic color from a deliberate one.

Files live beside the per-repo view overrides under `~/.switchbard/views/`: `<repo>.resume` for the existing restart record and `<repo>.history.json` for history. Resume is capped at 1 MiB. Unreadable or unsupported files are preserved, and errors appear in the terminal status or on exit. Repair the indicated file and reopen to resume saving. If another session changed a file, this session preserves that version and asks you to reopen rather than silently overwriting it. File locks and atomic replacement prevent partial records from becoming authoritative.
