# SBT repository setup

Running `sbt` means the terminal application is installed. A repository may still have no configured workspace. Setup registers its stable identity, activates all six native record kinds, and saves its initial configuration atomically in Switchbard's centralized database. It creates no backlog folder. SQLite is the current adapter; frontends call the backend-neutral core onboarding contract rather than inspect storage files or issue SQL.

Run `sbt init` to configure the current repository without opening the UI. Use `sbt --repo /path/to/repo init --yes` for automation, or `sbt --setup --yes` to configure and open the UI. The ordinary first launch offers setup with a default No. Explicit setup also asks for consent unless `--yes` is supplied.

After consent, setup explains task naming with an example such as `TASK-1`, and explains statuses as work stages. Enter accepts suggested settings, `c` customizes, and `n` cancels. Custom setup asks for an ID prefix and comma-separated stages, then shows a readable summary and asks for final confirmation. Blank fields accept displayed suggestions. The first stage is where new tasks start. Labels containing a comma can be supplied using repeated `--status` flags; a blank interactive stages answer retains those labels exactly. `Done` marks a task finished; other names are work stages, and a custom name such as `Finished` does not acquire Done's behavior.

```sh
sbt init --yes --task-prefix IW --status Inbox --status Doing --status Done
sbt --setup --yes --task-prefix IW --status Inbox --status Doing --status Done
```

Both setup routes accept `--task-prefix` and repeated `--status` flags. `--yes` accepts supplied settings or suggested defaults without prompts. Flags without setup are rejected. Prefixes use 1–24 ASCII letters, digits, hyphens or underscores and normalize to uppercase; they cannot start with a hyphen. Workflows have 1–20 distinct case-insensitive labels, each up to 64 Unicode characters (256 bytes). Interactive invalid values allow three correction attempts. Input is bounded; EOF at any prompt cancels without registration. No input terminal requires `--yes`.

Repository path defaults to the current directory and can be overridden with `--repo`. Theme, accounts, collaboration exports and migration policy stay out of first-launch setup. Existing legacy and mixed-authority workspaces continue opening; fresh setup never silently imports or hides their data. Use the reviewed `sb storage migrate` process to migrate them. Repeated setup retains records and configuration; identical explicit settings succeed, while conflicting settings refuse overwrite.

| State or stress | Behavior and evidence |
| --- | --- |
| Registered central repo without backlog | Opens through the real App; custom task creation and status change render and persist centrally |
| Interactive first launch | Consent default No, followed by explained settings choice; real PTY tests |
| Suggested or custom settings | Atomic all-kind activation and serialized config; CLI and real App tests |
| No, blank consent, final decline or EOF | Cancellation without registration; PTY tests at consent and settings/prefix prompts |
| Invalid prefix or duplicate stages | Readable correction; PTY recovery and no-write CLI tests |
| Noninteractive launch | Actionable `sbt init --yes` guidance, no writes; CLI test |
| Repeated or conflicting setup | Identity/sequence retained; matching overrides succeed and conflicting overrides refuse; CLI tests |
| Legacy or mixed registration | Explicit init refuses with migration guidance; CLI tests |
| Corrupt or missing established database | Recovery error, no fallback or overwrite; CLI tests |
| Invalid flags or directory | Validation before database creation |
| Long paths, narrow/short terminals | Ordinary terminal wrapping and scrollback; no alternate screen during prompts |
| Pointer, touch, zoom, theme states | Not applicable to line-based confirmation |
| Binary absent | Shell cannot run onboarding; use supported installation route |

If the shell reports `sbt: command not found`, install Switchbard's `sb` and `sbt` binaries using [the public TUI installation guide](INSTALL-TUI.md), ensure the install directory is on `PATH`, then run `sbt init`. The source `mise run install` route and opt-in main auto-install loop are developer workflows, separate from public release installation. The binary cannot detect its own absence; this differs from an unconfigured repository.

Arbitrary external editors cannot join Switchbard's repository lock. Empty inventories and branch/worktree digests are rechecked inside the write transaction immediately before activation, but a file written after that check remains a migration exception rather than being imported. Registration, six authorities, and configuration share one database transaction; failure rolls back those records. Interruption before consent writes nothing, and interrupted transactions rely on database rollback. Concurrent setup is serialized by the repository identity lock.

The original nine onboarding tests passed before customization. The expanded twelve tests pass after customization, including real PTY default/custom choices, invalid-entry recovery, cancellation and EOF, safe quote/comma label retention after native standard-status additions, and rendered custom task creation (`IW-1`, initially Inbox) followed by Done. Dedicated linked-worktree refusal, concurrent setup, signal-during-transaction and fault-injection tests remain gaps. Live owner data and installation are outside this implementation's verification scope.
