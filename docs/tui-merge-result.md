# Quiet merge results

Owner screenshot shows the core readback receipt dumped into the footer. Success should identify the merged PR only, never repository paths, commit hashes or verification prose. Core receipts and the retained result remain authoritative and unchanged.

| State | Evidence |
| --- | --- |
| Confirmed single merge; no target available | Real app `pr_merge` rendered at 40x8, 100x24 and 160x40; concise success, no internal receipt alert |
| Rejected or unknown result | Existing merge tests; retain actionable errors, never imply success |
| Bulk continuation, interruption, refresh | Existing bulk merge tests; queue and guards unchanged |
| Keyboard navigation and dismissal | Existing notification and page tests |
| Remote latency/offline/permissions | Core operation unchanged; no new remote writes in tests |
| Touch/zoom | N/A terminal surface |

Native owner visual confirmation and Linux CI remain separate gaps.
