# Check your setup and connect Claude Code or Codex

These commands are in current source and the upcoming v0.4.0-alpha.2 release; v0.4.0-alpha.1 predates them. Existing `sbt` first launch still prompts for workspace setup. Experienced users can continue that path without running diagnostics or installing skills.

## Diagnose before configuring

From your Git repository:

```sh
sbt doctor
sbt doctor --json
sbt --repo /path/to/repo doctor --github
```

The default check is local. `--github` additionally checks authentication, canonical repository access and open pull-request read access through `gh`, using its active account or environment token. It makes read requests and performs no GitHub writes. Check results include a status and a next action. Exit code 0 means no required local check failed; workspace setup may still remain; exit code 1 means a required local check failed. Optional GitHub or coding-agent failures still return 0, so scripts must inspect their individual statuses in JSON. Follow required repairs before setup; optional GitHub and coding-agent omissions do not block local task use. A successful diagnostic is readiness evidence, not proof every workflow is bug-free.

SQLite is compiled into `sb` and `sbt`. No SQL server, database account, or SQLite command-line utility is needed. Workspace setup creates and initializes the local database after consent. Doctor does not create a workspace, initialize or migrate the database, or alter storage authority. Destination checks use permissions and existing file access without creating a probe file; quotas and some ACL behavior remain unverified. It does not install dependencies or log in for you. Missing Git is repaired with your platform's Git installation; missing optional `gh` can be repaired before `gh auth login --hostname github.com --web`. See [GitHub permissions](INSTALL-TUI.md#connect-github-optional). Corrupt or inaccessible established storage needs investigation and recovery, never deleting it to make setup pass. Existing WAL-mode databases are not opened because inspection could create sidecar files. They produce a required failure with health and workspace registration unverified; investigate the reported storage state before setup. GitHub checks do not prove checks/review visibility, merge permissions, organization SSO policy or every token scope.

## Give your agent the setup request

```sh
sbt agent-prompt
```

Paste the output into a Claude Code or Codex session. It asks the agent to confirm the repository, run diagnostics, preserve existing data, offer default/custom workspace setup, and verify task creation. Optional integrations remain your choice. The agent should ask before OS installs or shell changes; you complete authentication. The prompt does not launch an agent, read credentials, or grant new authority.

## Install the shared task skill

```sh
sbt skill show
sbt skill install --agent both
# Or choose one:
sbt skill install --agent claude
sbt skill install --agent codex
```

The exact skill and interface metadata are embedded in the binary from [skills/switchbard](../skills/switchbard/SKILL.md); no download is required. Install writes personal files at:

- Claude Code: `~/.claude/skills/switchbard/SKILL.md`
- Codex: `~/.agents/skills/switchbard/SKILL.md`

Use `/switchbard` in Claude Code or `$switchbard` in Codex. If it is absent, restart the session. Personal skills apply to local sessions; remote/cloud environments need their own installation. Managed policies or repository-specific skills can affect discovery and precedence. See [Claude Code skill locations](https://code.claude.com/docs/en/skills#where-skills-live) and [Codex customization](https://learn.chatgpt.com/docs/customization/overview).

Installing identical files is safe to repeat. Differing custom files are refused by default before writing either agent's files; inspect and back them up before explicitly choosing `--replace`. Pre-existing symlinks in destination paths are refused, and paths are rechecked before writes. This installer assumes your personal skill directory is not being rearranged concurrently; it does not provide a race-free directory-handle sandbox against another local process. File writes are atomic individually; an I/O failure can leave a subset installed, so read the error and rerun after fixing access. Delete only the installed skill directory to remove this skill; preserve custom additions. No agent settings, hooks, credentials, OS packages, or repository data are changed by skill installation.

The skill teaches CLI task reading, criteria, progress notes and handoffs. Both agents can use `sb`; it does not provide native Codex hooks or accurate Codex live-claim attribution. Checked criteria, release, merge and Done remain separate facts. See [agent workflows](agent-workflows.md).
