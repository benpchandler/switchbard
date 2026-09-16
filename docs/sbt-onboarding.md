# SBT repository setup

Running `sbt` means the terminal application is installed. A repository may still have no configured SBT workspace. Setup registers its stable identity and activates all six native record kinds in Switchbard's centralized database. It creates no backlog folder. SQLite is the current adapter; frontend status and setup calls use the core onboarding contract rather than inspect storage files or issue SQL.

Run `sbt init` to configure the current repository without opening the UI. Use `sbt --repo /path/to/repo init --yes` for automation, or `sbt --setup --yes` to configure and open the UI. The ordinary first launch offers setup with a default No. Explicit setup also asks for confirmation unless `--yes` is supplied. No input terminal requires `--yes`; EOF and a negative answer cancel without writes.

The only setup question is whether to register the displayed repository using the central database. Repository path defaults to the current directory and can be overridden with `--repo`. Theme, task prefix, accounts, collaboration exports and migration policy are unnecessary first-launch decisions and stay out of this flow. Existing legacy workspaces continue opening; fresh setup never silently imports or hides their data. Use the reviewed `sb storage migrate` process to migrate them.

| State or stress | Expected behavior | Evidence |
| --- | --- | --- |
| Registered repository without backlog | Opens normally | Rendered App test and 120x30 raw-terminal probe |
| Unconfigured repository, interactive | Explains missing workspace and asks once, default No | PTY test |
| Confirmation Yes | Atomic registration and six central authorities | CLI integration test |
| No, blank or EOF | Cancels without registration | PTY test |
| Noninteractive launch | Actionable `sbt init --yes` guidance; no writes | CLI integration test |
| Repeated setup | Same identity and records retained | CLI integration test |
| Legacy files in any linked worktree | Refuses fresh setup; migration guidance | Isolated Git/worktree probe |
| Corrupt or missing established database | Recovery error, no fallback | CLI integration test |
| Unknown flags or missing path | Clap/path error before setup | CLI integration test |
| Binary absent (`command not found`) | Shell cannot run onboarding; install both sb/sbt using the supported installer | Documented installation route |

If the shell reports `sbt: command not found`, install Switchbard's `sb` and `sbt` binaries using the repository's supported `mise run install` route, then run `sbt init`. The binary cannot detect its own absence; this differs from an unconfigured repository.

Evidence entries above define the required checks; they are not claims of completed tests. Final verification results belong in the delivery report. Live owner data and installation are outside this implementation's verification scope.

The confirmation uses ordinary terminal text and wraps long paths at the terminal edge; it has no alternate-screen, pointer, touch, zoom, or theme-dependent interaction. Narrow and short terminals retain scrollback. Arbitrary external editors cannot join Switchbard's repository lock: empty inventories and branch/worktree digests are rechecked inside the activation transaction before authority writes, but an external file written after that check remains a migration exception rather than being imported. Registration, six authorities, and default config share one database transaction; a failed transaction rolls back those records. Interruption before confirmation writes nothing, and interrupted database transactions rely on database rollback. Dedicated signal-during-transaction and fault-injection tests are explicit gaps. Concurrent setup is serialized by the repository identity lock; repeated setup retains existing records.

Verified: nine CLI/application tests pass, including real PTY acceptance, blank/negative/EOF cancellation, default status selection after central task creation, all-kind activation, idempotence, legacy refusal, corruption, established database loss, and mixed registration. Additional isolated probes verified linked-worktree legacy refusal, simultaneous setup, and a central-only 120x30 raw-terminal launch through rendered Tasks and normal quit. Raw terminal output is retained at `/tmp/sbt-onboarding-terminal-proof.txt`; full gate output is retained at `/tmp/sbt-onboarding-ci-final.log`. Narrow-terminal confirmation, signal-during-activation, and fault injection remain explicit evidence gaps.
