use std::time::Duration;

use switchbard_gui::perf::{FrameSample, PerfStats};

fn sample(total_ms: u64, workspace_ms: u64, rows: usize) -> FrameSample {
    FrameSample {
        total: Duration::from_millis(total_ms),
        workspace: Duration::from_millis(workspace_ms),
        rows_rendered: rows,
        ..Default::default()
    }
}

#[test]
fn perf_stats_reports_percentiles_and_maxima() {
    let mut stats = PerfStats::new(128);
    for ms in 1..=100 {
        stats.record(sample(ms, ms / 2, ms as usize));
    }

    let summary = stats.summary().unwrap();

    assert_eq!(summary.frames, 100);
    assert_eq!(summary.total.p50_ms, 50.0);
    assert_eq!(summary.total.p95_ms, 95.0);
    assert_eq!(summary.total.p99_ms, 99.0);
    assert_eq!(summary.total.max_ms, 100.0);
    assert_eq!(summary.rows_rendered_max, 100);
}

#[test]
fn perf_stats_respects_the_rolling_capacity() {
    let mut stats = PerfStats::new(3);
    stats.record(sample(10, 1, 1));
    stats.record(sample(20, 1, 2));
    stats.record(sample(30, 1, 3));
    stats.record(sample(40, 1, 4));

    let summary = stats.summary().unwrap();

    assert_eq!(summary.frames, 3);
    assert_eq!(summary.total.p50_ms, 30.0);
    assert_eq!(summary.total.max_ms, 40.0);
    assert_eq!(summary.rows_rendered_max, 4);
}
