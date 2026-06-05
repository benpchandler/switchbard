# Switchbard Perf Ledger

Tracked perf records live in `docs/perf/runs/*.json`. They are compact summaries
of perf CSVs, not raw frame logs.

Raw captures should stay out of git. Put them in `/tmp` or `docs/perf/raw/`
(`docs/perf/raw/` and perf CSVs under `docs/perf/` are ignored).

To record a run:

```sh
SWITCHBARD_PERF=1 SWITCHBARD_PERF_LOG=/tmp/switchbard-perf.csv \
  target/release/Switchbard.app/Contents/MacOS/Switchbard

python3 scripts/perf-summary.py \
  --csv /tmp/switchbard-perf.csv \
  --out docs/perf/runs/YYYY-MM-DD-scenario.json \
  --label "Servers scroll smoke" \
  --scenario servers-scroll-smoke \
  --filter servers
```
