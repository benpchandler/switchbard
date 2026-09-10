//! The handoff a self-restart carries: what the running build was looking at,
//! written into the environment of the build that replaces it.
//!
//! One build writes this and a *different* build reads it, so the record is a
//! named-field object, not a positional tuple: a build that gained a field
//! still reads an older record (missing fields take their defaults), and a
//! build that lost one still reads a newer record (unknown fields are
//! ignored). A positional tuple has neither property, which is how TASK-173
//! happened - a record the arriving build could not parse was dropped in
//! silence and the user landed back on saved slot 1.
//!
//! The other half of that rule is [`Restored`]: a record we cannot read is a
//! reportable outcome, never a quiet fall-back to defaults.

use serde::{Deserialize, Serialize};

/// The prefix naming the record format. Bump it only for a change no
/// `#[serde(default)]` field can absorb; readers reject an unknown prefix
/// rather than guessing at the bytes behind it.
const PREFIX: &str = "sbt-resume-1=";
/// The positional format shipped before TASK-173, still read on the way in so
/// a build carrying this fix can take over from one that does not.
const LEGACY_PREFIX: &str = "pages=";

/// What the departing build was looking at. Every field defaults, so adding
/// one here never breaks a build that has not learned about it yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResumeRecord {
    /// Which page was in front: the Tasks page is the default.
    pub pr_page: bool,
    #[serde(default)]
    pub inbox_page: bool,
    pub task_slot: usize,
    /// The live task view as its Lua record, the same text a saved slot holds.
    pub task_view: String,
    pub task_selected: usize,
    pub pr_slot: usize,
    pub pr_view: String,
    pub pr_selected: usize,
    /// The selected PR's identity, so the row is found again after a refetch.
    pub pr_id: Option<String>,
}

impl ResumeRecord {
    /// The environment value handed to the replacement build.
    pub fn encode(&self) -> String {
        format!(
            "{PREFIX}{}",
            serde_json::to_string(self).expect("resume record serialization")
        )
    }
}

/// What reading the handed-over value produced. `Absent` is an ordinary cold
/// start; `Unreadable` is a record we were given and could not use, which the
/// caller must surface rather than silently opening on saved slot 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restored {
    Absent,
    Record(ResumeRecord),
    Unreadable,
}

/// Reads either format. Anything else is `Unreadable`: an empty or malformed
/// value still means a build handed us state we owe the user an answer about.
pub fn decode(value: Option<&str>) -> Restored {
    let Some(value) = value else {
        return Restored::Absent;
    };
    if let Some(json) = value.strip_prefix(PREFIX) {
        return match serde_json::from_str(json) {
            Ok(record) => Restored::Record(record),
            Err(_) => Restored::Unreadable,
        };
    }
    if let Some(record) = value.strip_prefix(LEGACY_PREFIX) {
        return decode_legacy(record);
    }
    Restored::Unreadable
}

/// The pre-TASK-173 positional tuple, in both the arities it shipped with.
fn decode_legacy(record: &str) -> Restored {
    type Eight = (
        bool,
        usize,
        usize,
        String,
        usize,
        String,
        usize,
        Option<String>,
    );
    let parsed = serde_json::from_str::<Eight>(record).or_else(|_| {
        serde_json::from_str::<(bool, usize, usize, String, usize, String, usize)>(record).map(
            |(pr_page, task_slot, task_selected, task_view, pr_slot, pr_view, pr_selected)| {
                (
                    pr_page,
                    task_slot,
                    task_selected,
                    task_view,
                    pr_slot,
                    pr_view,
                    pr_selected,
                    None,
                )
            },
        )
    });
    match parsed {
        Ok((
            pr_page,
            task_slot,
            task_selected,
            task_view,
            pr_slot,
            pr_view,
            pr_selected,
            pr_id,
        )) => Restored::Record(ResumeRecord {
            pr_page,
            inbox_page: false,
            task_slot,
            task_view,
            task_selected,
            pr_slot,
            pr_view,
            pr_selected,
            pr_id,
        }),
        Err(_) => Restored::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ResumeRecord {
        ResumeRecord {
            pr_page: true,
            inbox_page: false,
            task_slot: 2,
            task_view: "{ filter = \"status:!done\", group = \"project\" }".into(),
            task_selected: 7,
            pr_slot: 1,
            pr_view: "{ filter = \"\" }".into(),
            pr_selected: 3,
            pr_id: Some("PR_9".into()),
        }
    }

    #[test]
    fn a_record_survives_its_own_round_trip() {
        assert_eq!(decode(Some(&record().encode())), Restored::Record(record()));
    }

    #[test]
    fn a_newer_builds_extra_field_is_ignored_rather_than_losing_the_view() {
        let mut json: serde_json::Value =
            serde_json::from_str(record().encode().strip_prefix(PREFIX).expect("prefix"))
                .expect("json");
        json["a_field_this_build_has_never_heard_of"] = serde_json::json!("whatever");
        let encoded = format!("{PREFIX}{json}");
        assert_eq!(decode(Some(&encoded)), Restored::Record(record()));
    }

    #[test]
    fn an_older_builds_missing_field_takes_its_default_rather_than_losing_the_view() {
        let encoded = format!("{PREFIX}{{\"task_slot\":2,\"task_selected\":7}}");
        assert_eq!(
            decode(Some(&encoded)),
            Restored::Record(ResumeRecord {
                task_slot: 2,
                task_selected: 7,
                ..ResumeRecord::default()
            })
        );
    }

    #[test]
    fn both_legacy_arities_still_read() {
        let eight = "pages=[true,2,7,\"{}\",1,\"{}\",3,\"PR_9\"]";
        assert_eq!(
            decode(Some(eight)),
            Restored::Record(ResumeRecord {
                pr_page: true,
                inbox_page: false,
                task_slot: 2,
                task_view: "{}".into(),
                task_selected: 7,
                pr_slot: 1,
                pr_view: "{}".into(),
                pr_selected: 3,
                pr_id: Some("PR_9".into()),
            })
        );
        let seven = "pages=[false,1,4,\"{}\",0,\"{}\",2]";
        assert_eq!(
            decode(Some(seven)),
            Restored::Record(ResumeRecord {
                task_slot: 1,
                task_view: "{}".into(),
                task_selected: 4,
                pr_view: "{}".into(),
                pr_selected: 2,
                ..ResumeRecord::default()
            })
        );
    }

    #[test]
    fn nothing_handed_over_is_a_cold_start_but_gibberish_is_reportable() {
        assert_eq!(decode(None), Restored::Absent);
        assert_eq!(decode(Some("")), Restored::Unreadable);
        assert_eq!(decode(Some("pages=not-json")), Restored::Unreadable);
        assert_eq!(decode(Some("sbt-resume-9={}")), Restored::Unreadable);
    }
}
