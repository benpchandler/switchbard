//! UTC calendar-day categories from authoritative domain timestamps. Overlapping
//! ranges compose through the ordinary matcher and paint precedence, never a
//! separate date-rule evaluator. First category is the compact display bucket.
use chrono::Utc;
use switchbard_core::{parse_backlog_day, PrLifecycle, PrListRow};

pub const BUCKETS: &[&str] = &[
    "today",
    "yesterday",
    "last 7 days",
    "last 30 days",
    "older",
    "future",
    "missing",
];

pub fn filed(value: Option<&str>) -> Vec<String> {
    buckets(value.and_then(parse_backlog_day), today())
}

pub fn merged(row: &PrListRow) -> Vec<String> {
    let day = row
        .merged_at
        .filter(|_| row.lifecycle == PrLifecycle::Merged)
        .map(|date| date.timestamp().div_euclid(86_400));
    buckets(day, today())
}

pub(crate) fn today() -> i64 {
    Utc::now().timestamp().div_euclid(86_400)
}

fn buckets(day: Option<i64>, today: i64) -> Vec<String> {
    let Some(day) = day else {
        return vec!["missing".into()];
    };
    let age = today - day;
    let mut result = Vec::with_capacity(3);
    if age < 0 {
        result.push("future");
    }
    if age == 0 {
        result.push("today");
    }
    if age == 1 {
        result.push("yesterday");
    }
    if (0..7).contains(&age) {
        result.push("last 7 days");
    }
    if (0..30).contains(&age) {
        result.push("last 30 days");
    }
    if age >= 30 {
        result.push("older");
    }
    result.into_iter().map(str::to_string).collect()
}
