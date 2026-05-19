# Hive

Beekeeper-style desktop app for local dev workflows. Add a repo, start/stop its services, see what's actually running and on what SHA.

**Status:** pre-implementation. See `docs/design.md` for contracts, components, and workflows.

## Stack
- Rust core (`hive-core` crate) — domain types and OS interaction, no UI deps
- Tauri shell (`hive-tauri` crate) — bridges core to frontend
- TypeScript + Vite frontend — single-window UI
- SQLite via `rusqlite` — local state at `~/Library/Application Support/hive/`
