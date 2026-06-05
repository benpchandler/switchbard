//! Lightweight runtime performance telemetry.
//!
//! Enabled only when `SWITCHBARD_PERF` is set. The goal is measurement, not a
//! permanent dashboard: collect enough frame/render timing and row-count data
//! to compare scrolling changes without attaching Instruments every time.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DEFAULT_CAPACITY: usize = 600;
const DEFAULT_LOG_PATH: &str = "/tmp/switchbard-perf.csv";
const LOG_FLUSH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameSample {
    pub total: Duration,
    pub top_bar: Duration,
    pub sidebar: Duration,
    pub central: Duration,
    pub workspace: Duration,
    pub onboarding: Duration,
    pub rows_rendered: usize,
    pub rows_expanded: usize,
    pub services_rendered: usize,
    pub listeners_rendered: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DurationSummary {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerfSummary {
    pub frames: usize,
    pub total: DurationSummary,
    pub workspace: DurationSummary,
    pub rows_rendered_max: usize,
    pub rows_expanded_max: usize,
    pub services_rendered_max: usize,
    pub listeners_rendered_max: usize,
}

pub struct PerfStats {
    capacity: usize,
    samples: VecDeque<FrameSample>,
}

impl PerfStats {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            samples: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    pub fn record(&mut self, sample: FrameSample) {
        while self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn summary(&self) -> Option<PerfSummary> {
        if self.samples.is_empty() {
            return None;
        }
        Some(PerfSummary {
            frames: self.samples.len(),
            total: summarize_duration(self.samples.iter().map(|sample| sample.total)),
            workspace: summarize_duration(self.samples.iter().map(|sample| sample.workspace)),
            rows_rendered_max: self
                .samples
                .iter()
                .map(|sample| sample.rows_rendered)
                .max()
                .unwrap_or(0),
            rows_expanded_max: self
                .samples
                .iter()
                .map(|sample| sample.rows_expanded)
                .max()
                .unwrap_or(0),
            services_rendered_max: self
                .samples
                .iter()
                .map(|sample| sample.services_rendered)
                .max()
                .unwrap_or(0),
            listeners_rendered_max: self
                .samples
                .iter()
                .map(|sample| sample.listeners_rendered)
                .max()
                .unwrap_or(0),
        })
    }
}

pub struct PerfSession {
    stats: PerfStats,
    current: FrameSample,
    frame_index: u64,
    writer: Option<BufWriter<File>>,
    last_flush: Instant,
}

impl PerfSession {
    pub fn from_env() -> Option<Self> {
        if !env_enabled("SWITCHBARD_PERF") {
            return None;
        }
        let path = std::env::var_os("SWITCHBARD_PERF_LOG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_LOG_PATH));
        Some(Self::new(DEFAULT_CAPACITY, Some(path)))
    }

    pub fn new(capacity: usize, log_path: Option<PathBuf>) -> Self {
        let writer = log_path.as_deref().and_then(open_perf_log);
        Self {
            stats: PerfStats::new(capacity),
            current: FrameSample::default(),
            frame_index: 0,
            writer,
            last_flush: Instant::now(),
        }
    }

    pub fn begin_frame(&mut self) {
        self.current = FrameSample::default();
    }

    pub fn record_top_bar(&mut self, duration: Duration) {
        self.current.top_bar = duration;
    }

    pub fn record_sidebar(&mut self, duration: Duration) {
        self.current.sidebar = duration;
    }

    pub fn record_central(&mut self, duration: Duration) {
        self.current.central = duration;
    }

    pub fn record_workspace(&mut self, duration: Duration) {
        self.current.workspace = duration;
    }

    pub fn record_onboarding(&mut self, duration: Duration) {
        self.current.onboarding = duration;
    }

    pub fn count_worktree_row(&mut self, expanded: bool, services: usize, listeners: usize) {
        self.current.rows_rendered += 1;
        if expanded {
            self.current.rows_expanded += 1;
        }
        self.current.services_rendered += services;
        self.current.listeners_rendered += listeners;
    }

    pub fn finish_frame(&mut self, total: Duration) {
        self.current.total = total;
        let sample = self.current;
        self.stats.record(sample);
        self.write_sample(sample);
        self.frame_index += 1;
    }

    pub fn summary(&self) -> Option<PerfSummary> {
        self.stats.summary()
    }
}

impl PerfSummary {
    pub fn overlay_text(&self) -> String {
        format!(
            "perf {}f  frame p50 {:.1} p95 {:.1} p99 {:.1} max {:.1} ms\nworkspace p95 {:.1} max {:.1} ms  rows max {} expanded {} svc {} listeners {}",
            self.frames,
            self.total.p50_ms,
            self.total.p95_ms,
            self.total.p99_ms,
            self.total.max_ms,
            self.workspace.p95_ms,
            self.workspace.max_ms,
            self.rows_rendered_max,
            self.rows_expanded_max,
            self.services_rendered_max,
            self.listeners_rendered_max,
        )
    }
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !matches!(value.as_str(), "" | "0" | "false" | "off" | "no")
        })
        .unwrap_or(false)
}

fn open_perf_log(path: &Path) -> Option<BufWriter<File>> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .ok()?;
    let mut writer = BufWriter::new(file);
    let _ = writeln!(
        writer,
        "frame,total_ms,top_bar_ms,sidebar_ms,central_ms,workspace_ms,onboarding_ms,rows,expanded_rows,services,listeners"
    );
    Some(writer)
}

impl PerfSession {
    fn write_sample(&mut self, sample: FrameSample) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let _ = writeln!(
            writer,
            "{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{}",
            self.frame_index,
            ms(sample.total),
            ms(sample.top_bar),
            ms(sample.sidebar),
            ms(sample.central),
            ms(sample.workspace),
            ms(sample.onboarding),
            sample.rows_rendered,
            sample.rows_expanded,
            sample.services_rendered,
            sample.listeners_rendered,
        );
        if self.last_flush.elapsed() >= LOG_FLUSH_INTERVAL {
            let _ = writer.flush();
            self.last_flush = Instant::now();
        }
    }
}

fn summarize_duration(values: impl Iterator<Item = Duration>) -> DurationSummary {
    let mut values_ms: Vec<f64> = values.map(ms).collect();
    values_ms.sort_by(f64::total_cmp);
    DurationSummary {
        p50_ms: percentile(&values_ms, 50.0),
        p95_ms: percentile(&values_ms, 95.0),
        p99_ms: percentile(&values_ms, 99.0),
        max_ms: *values_ms.last().unwrap_or(&0.0),
    }
}

fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let rank = ((percentile / 100.0) * sorted_values.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted_values.len() - 1);
    sorted_values[index]
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
