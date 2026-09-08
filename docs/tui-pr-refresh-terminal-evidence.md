# PR refresh terminal evidence

Real Ratatui TestBackend output from `tests/pr_refresh.rs`, including read-only live GitHub success followed by a real invalid-repository refresh failure. Narrow and wide containers are asserted by the tests; compact captures are retained here.

## loading

```text
loading 40x8
  Tasks   [Pull Requests]  tab switch pa
┌ Pull Requests ───────────────────────┐
│refreshing · Loading pull requests... │
│                                      │
│                                      │
│                                      │
└──────────────────────────────────────┘
/ search · f filter · s sort · p paint ·
```

## live-success

```text
live-success 40x8
  Tasks   [Pull Requests]  tab switch pa
┌ Pull Requests ───────────────────────┐
│benpchandler/switchbard · Observed 60s│
│100/100 shown · PARTIAL · :more       │
│filter: all                           │
│                                      │
└──────────────────────────────────────┘
/ search · f filter · s sort · p paint ·
```

## visible-zero

```text
visible-zero 40x8
  Tasks   [Pull Requests]  tab switch pa
┌ Pull Requests ───────────────────────┐
│0s · Unavailable: GitHub read failed: │
│failed to run git: fatal: not a git   │
│repository (or any of the parent      │
│directories): .git. Use refresh to    │
└──────────────────────────────────────┘
/ search · f filter · s sort · p paint ·
```

## live-cached-refresh

```text
live-cached-refresh 40x8
  Tasks   [Pull Requests]  tab switch pa
┌ Pull Requests ───────────────────────┐
│benpchandler/swi · Observed refreshing│
│100/100 shown · PARTIAL · :more       │
│filter: all                           │
│                                      │
└──────────────────────────────────────┘
/ search · f filter · s sort · p paint ·
```

## live-cached-failure

```text
live-cached-failure 40x8
  Tasks   [Pull Requests]  tab switch pa
┌ Pull Requests ───────────────────────┐
│benpchandler/switchbard · STALE 60s   │
│100/100 shown · PARTIAL · :more       │
│filter: all                           │
│GitHub read failed: failed to run git:│
└──────────────────────────────────────┘
/ search · f filter · s sort · p paint ·
```
