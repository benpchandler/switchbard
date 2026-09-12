use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{
    de::{MapAccess, Visitor},
    Deserializer,
};
use std::collections::BTreeMap;

type Clock = BTreeMap<String, u64>;

pub(super) fn validate(clock: &Clock) -> Result<()> {
    ensure!(
        !clock.is_empty() && clock.len() <= 128,
        "invalid causal clock size"
    );
    for (replica, count) in clock {
        super::validate_uuid(replica)?;
        ensure!(
            *count > 0 && *count <= 9_007_199_254_740_991,
            "invalid causal clock counter"
        );
    }
    Ok(())
}

pub(crate) fn increment_clock(connection: &Connection, id: &str) -> Result<()> {
    let mut clock = load_optional(connection, id)?.unwrap_or_default();
    increment(connection, &mut clock)?;
    save(connection, id, &clock)
}

pub(super) fn increment(connection: &Connection, clock: &mut Clock) -> Result<()> {
    let replica: String =
        connection.query_row("SELECT replica_id FROM metadata", [], |row| row.get(0))?;
    let next = clock
        .get(&replica)
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .context("causal counter exhausted")?;
    clock.insert(replica, next);
    validate(clock)
}

pub(super) fn load(connection: &Connection, id: &str) -> Result<Clock> {
    load_optional(connection, id)?.context("document has no causal clock")
}

fn load_optional(connection: &Connection, id: &str) -> Result<Option<Clock>> {
    let raw: Option<String> = connection
        .query_row("SELECT clock FROM clocks WHERE record_id=?1", [id], |row| {
            row.get(0)
        })
        .optional()?;
    raw.map(|raw| Ok(serde_json::from_str(&raw)?)).transpose()
}

pub(super) fn save(connection: &Connection, id: &str, clock: &Clock) -> Result<()> {
    connection.execute("INSERT INTO clocks(record_id,clock) VALUES (?1,?2) ON CONFLICT(record_id) DO UPDATE SET clock=excluded.clock", params![id,serde_json::to_string(clock)?])?;
    Ok(())
}

pub(super) fn dominates(left: &Clock, right: &Clock) -> bool {
    right
        .iter()
        .all(|(id, count)| left.get(id).copied().unwrap_or(0) >= *count)
}

pub(super) fn join(left: &Clock, right: &Clock) -> Clock {
    let mut joined = left.clone();
    for (id, count) in right {
        joined
            .entry(id.clone())
            .and_modify(|old| *old = (*old).max(*count))
            .or_insert(*count);
    }
    joined
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Clock, D::Error> {
    struct ClockVisitor;
    impl<'de> Visitor<'de> for ClockVisitor {
        type Value = Clock;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unique replica clock map")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Clock, A::Error> {
            let mut clock = Clock::new();
            for _ in 0..=128 {
                let Some((id, count)) = map.next_entry::<String, u64>()? else {
                    return Ok(clock);
                };
                if clock.len() == 128 || clock.insert(id, count).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate replica or oversized clock",
                    ));
                }
            }
            Err(serde::de::Error::custom("oversized clock"))
        }
    }
    deserializer.deserialize_map(ClockVisitor)
}
