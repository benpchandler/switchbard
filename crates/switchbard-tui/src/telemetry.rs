//! Append-only local event log: what the user pressed, what it did, how long it took.
//! Feeds bug reports (the recent trail) and `sbt stats` (what is used, what is slow).

use std::collections::{BTreeMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const TRAIL_LEN: usize = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: u64,
    pub kind: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
}

pub struct Telemetry {
    sink: Option<File>,
    trail: VecDeque<Event>,
    render_ms: Vec<f64>,
    started: Instant,
}

pub fn default_log_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".switchbard").join("tui-events.jsonl"))
}

impl Telemetry {
    pub fn to_file(path: &Path) -> Telemetry {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let sink = OpenOptions::new().create(true).append(true).open(path).ok();
        let mut telemetry = Telemetry::with_sink(sink);
        telemetry.record("session_start", env!("CARGO_PKG_VERSION"));
        telemetry
    }

    pub fn in_memory() -> Telemetry {
        Telemetry::with_sink(None)
    }

    fn with_sink(sink: Option<File>) -> Telemetry {
        Telemetry {
            sink,
            trail: VecDeque::new(),
            render_ms: Vec::new(),
            started: Instant::now(),
        }
    }

    pub fn record(&mut self, kind: &str, detail: impl Into<String>) {
        self.push(Event {
            ts: unix_millis(),
            kind: kind.to_string(),
            detail: detail.into(),
            ms: None,
        });
    }

    pub fn record_timed(&mut self, kind: &str, detail: impl Into<String>, started: Instant) {
        self.push(Event {
            ts: unix_millis(),
            kind: kind.to_string(),
            detail: detail.into(),
            ms: Some(millis_since(started)),
        });
    }

    pub fn record_render(&mut self, started: Instant) {
        self.render_ms.push(millis_since(started));
    }

    pub fn trail(&self) -> Vec<String> {
        self.trail
            .iter()
            .map(|event| match event.ms {
                Some(ms) => format!("{} {} ({ms:.1}ms)", event.kind, event.detail),
                None => format!("{} {}", event.kind, event.detail),
            })
            .collect()
    }

    pub fn finish(&mut self) {
        let summary = format!(
            "frames={} render_p50={:.2}ms render_p95={:.2}ms",
            self.render_ms.len(),
            percentile(&mut self.render_ms.clone(), 0.50),
            percentile(&mut self.render_ms.clone(), 0.95),
        );
        self.record_timed("session_end", summary, self.started);
    }

    fn push(&mut self, event: Event) {
        if let Some(sink) = self.sink.as_mut() {
            if let Ok(line) = serde_json::to_string(&event) {
                let _ = writeln!(sink, "{line}");
            }
        }
        self.trail.push_back(event);
        if self.trail.len() > TRAIL_LEN {
            self.trail.pop_front();
        }
    }
}

/// Human summary of the log for `sbt stats`: usage, friction, and speed.
pub fn stats(path: &Path) -> anyhow::Result<String> {
    let file = File::open(path)?;
    let mut sessions = 0usize;
    let mut actions: BTreeMap<String, usize> = BTreeMap::new();
    let mut unbound: BTreeMap<String, usize> = BTreeMap::new();
    let mut errors = Vec::new();
    let mut session_ends = Vec::new();
    let mut slow_actions: Vec<(f64, String)> = Vec::new();
    for line in BufReader::new(file).lines() {
        let Ok(event) = serde_json::from_str::<Event>(&line?) else {
            continue;
        };
        match event.kind.as_str() {
            "session_start" => sessions += 1,
            "session_end" => session_ends.push(event.detail),
            "action" => {
                *actions.entry(event.detail.clone()).or_default() += 1;
                if let Some(ms) = event.ms.filter(|ms| *ms > 50.0) {
                    slow_actions.push((ms, event.detail));
                }
            }
            "unbound" => *unbound.entry(event.detail).or_default() += 1,
            "error" => errors.push(event.detail),
            _ => {}
        }
    }
    let mut out = format!("sessions: {sessions}\n\nactions:\n");
    let mut ranked: Vec<_> = actions.into_iter().collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (name, count) in ranked {
        out.push_str(&format!("  {count:>5}  {name}\n"));
    }
    if !unbound.is_empty() {
        out.push_str("\nunbound keys pressed (discoverability gaps):\n");
        let mut ranked: Vec<_> = unbound.into_iter().collect();
        ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        for (key, count) in ranked {
            out.push_str(&format!("  {count:>5}  {key}\n"));
        }
    }
    if !slow_actions.is_empty() {
        out.push_str("\nslow actions (>50ms):\n");
        slow_actions.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (ms, name) in slow_actions.iter().take(10) {
            out.push_str(&format!("  {ms:>7.1}ms  {name}\n"));
        }
    }
    if !errors.is_empty() {
        out.push_str(&format!("\nerrors ({}):\n", errors.len()));
        for error in errors.iter().rev().take(10) {
            out.push_str(&format!("  {error}\n"));
        }
    }
    out.push_str("\nrender per session:\n");
    for end in session_ends.iter().rev().take(10) {
        out.push_str(&format!("  {end}\n"));
    }
    Ok(out)
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn millis_since(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let index = ((samples.len() - 1) as f64 * fraction).round() as usize;
    samples[index]
}
