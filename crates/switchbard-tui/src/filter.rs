//! Entity-independent query parsing and predicates for search, facets and paint.
use crate::columns::Column;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Id,
    Status,
    Priority,
    Label,
    Project,
    Ball,
    Due,
    Goal,
    Title,
    Tasks,
    Checks,
    Review,
    Merge,
    Draft,
    Filed,
    Merged,
}

impl FilterField {
    fn parse(keyword: &str) -> Option<FilterField> {
        Some(match keyword {
            "id" => FilterField::Id,
            "status" | "lifecycle" => FilterField::Status,
            "pri" | "priority" => FilterField::Priority,
            "label" => FilterField::Label,
            "project" => FilterField::Project,
            "ball" => FilterField::Ball,
            "due" => FilterField::Due,
            "goal" => FilterField::Goal,
            "title" => FilterField::Title,
            "tasks" => FilterField::Tasks,
            "checks" => FilterField::Checks,
            "review" => FilterField::Review,
            "merge" => FilterField::Merge,
            "draft" => FilterField::Draft,
            "filed" | "created" => FilterField::Filed,
            "merged" | "merged_at" => FilterField::Merged,
            _ => return None,
        })
    }

    pub fn keyword(self) -> &'static str {
        match self {
            FilterField::Id => "id",
            FilterField::Status => "status",
            FilterField::Priority => "pri",
            FilterField::Label => "label",
            FilterField::Project => "project",
            FilterField::Ball => "ball",
            FilterField::Due => "due",
            FilterField::Goal => "goal",
            FilterField::Draft => "draft",
            FilterField::Merge => "merge",
            FilterField::Review => "review",
            FilterField::Checks => "checks",
            FilterField::Tasks => "tasks",
            FilterField::Title => "title",
            FilterField::Filed => "filed",
            FilterField::Merged => "merged",
        }
    }

    /// The column this field reads from.
    pub fn column(self) -> Column {
        match self {
            FilterField::Id => Column::Id,
            FilterField::Status => Column::Status,
            FilterField::Priority => Column::Priority,
            FilterField::Label => Column::Labels,
            FilterField::Project => Column::Project,
            FilterField::Ball => Column::Ball,
            FilterField::Due => Column::Due,
            FilterField::Goal => Column::Goal,
            FilterField::Draft => Column::Draft,
            FilterField::Merge => Column::Merge,
            FilterField::Review => Column::Review,
            FilterField::Checks => Column::Checks,
            FilterField::Tasks => Column::Tasks,
            FilterField::Title => Column::Title,
            FilterField::Filed => Column::Filed,
            FilterField::Merged => Column::Merged,
        }
    }
}

/// A relative-day comparator for `due:<=7d`-style terms; the day delta is
/// `due_date - today` (negative when overdue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DueCmp {
    Le,
    Lt,
    Ge,
    Gt,
}

impl DueCmp {
    fn matches(self, delta: i64, threshold: i64) -> bool {
        match self {
            DueCmp::Le => delta <= threshold,
            DueCmp::Lt => delta < threshold,
            DueCmp::Ge => delta >= threshold,
            DueCmp::Gt => delta > threshold,
        }
    }
}

/// One word of a filter. `pri:high,medium` is one term matching either value;
/// `status:!done` hides a value; a bare word searches id and title.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Text(String),
    AnyOf(FilterField, Vec<String>),
    Not(FilterField, String),
    /// `due:<=7d` / `due:<7d` / `due:>=7d` / `due:>7d` — relative to today,
    /// computed off the same clock `date_fields::today()` uses.
    DueRelative(DueCmp, i64),
}

impl Term {
    fn parse(word: &str) -> Term {
        let lower = word.to_lowercase();
        let Some((keyword, value)) = lower.split_once(':') else {
            return Term::Text(lower);
        };
        let Some(field) = FilterField::parse(keyword) else {
            return Term::Text(lower);
        };
        if field == FilterField::Due {
            if let Some(term) = parse_due_relative(value) {
                return term;
            }
        }
        match value.strip_prefix('!') {
            Some(hidden) => Term::Not(field, loose(hidden)),
            None => Term::AnyOf(field, value.split(',').map(loose).collect()),
        }
    }

    fn field(&self) -> Option<FilterField> {
        match self {
            Term::Text(_) => None,
            Term::AnyOf(field, _) | Term::Not(field, _) => Some(*field),
            Term::DueRelative(..) => Some(FilterField::Due),
        }
    }

    fn allows(&self, values: &[String]) -> bool {
        match self {
            Term::Text(_) => true,
            // `id:` is exact so a painted TASK-13 never also paints TASK-130.
            Term::AnyOf(FilterField::Id | FilterField::Tasks, wanted) => values
                .iter()
                .any(|value| wanted.iter().any(|want| loose(value) == *want)),
            // `due:none` matches a task with no due date, which carries no
            // value at all rather than a sentinel string.
            Term::AnyOf(FilterField::Due, wanted) if values.is_empty() => {
                wanted.iter().any(|want| want == "none")
            }
            Term::AnyOf(_, wanted) => values
                .iter()
                .any(|value| wanted.iter().any(|want| loose(value).contains(want))),
            Term::Not(_, hidden) => !values.iter().any(|value| loose(value) == *hidden),
            Term::DueRelative(cmp, threshold) => values.iter().any(|value| {
                due_day_delta(value).is_some_and(|delta| cmp.matches(delta, *threshold))
            }),
        }
    }
}

/// Parses `<=Nd`, `<Nd`, `>=Nd`, `>Nd`; anything else (an exact date, `none`)
/// falls through to the ordinary `AnyOf`/`Not` handling.
fn parse_due_relative(value: &str) -> Option<Term> {
    let (cmp, rest) = if let Some(rest) = value.strip_prefix("<=") {
        (DueCmp::Le, rest)
    } else if let Some(rest) = value.strip_prefix(">=") {
        (DueCmp::Ge, rest)
    } else if let Some(rest) = value.strip_prefix('<') {
        (DueCmp::Lt, rest)
    } else if let Some(rest) = value.strip_prefix('>') {
        (DueCmp::Gt, rest)
    } else {
        return None;
    };
    let days: i64 = rest.strip_suffix('d')?.parse().ok()?;
    Some(Term::DueRelative(cmp, days))
}

/// `due_date - today`, in whole days; `None` for an unparseable or absent value.
fn due_day_delta(value: &str) -> Option<i64> {
    let day = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(0, 0, 0)?
        .and_utc()
        .timestamp()
        .div_euclid(86_400);
    Some(day - crate::date_fields::today())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    terms: Vec<Term>,
}

impl Filter {
    pub fn parse(text: &str) -> Filter {
        Filter {
            terms: text.split_whitespace().map(Term::parse).collect(),
        }
    }

    pub fn fields(&self) -> impl Iterator<Item = FilterField> + '_ {
        self.terms.iter().filter_map(Term::field)
    }

    pub fn matches_row(&self, row: &impl crate::column_values::ColumnValues) -> bool {
        let text = row.text();
        let text: Vec<&str> = text.iter().map(String::as_str).collect();
        self.matches_values(&text, |field| row.values(field.column()))
    }

    /// Shared filter grammar; each page supplies the values its fields mean.
    pub fn matches_values(
        &self,
        text: &[&str],
        values: impl Fn(FilterField) -> Vec<String>,
    ) -> bool {
        self.terms.iter().all(|term| match term {
            Term::Text(needle) => text
                .iter()
                .any(|value| value.to_lowercase().contains(needle)),
            Term::AnyOf(field, _) | Term::Not(field, _) => term.allows(&values(*field)),
            Term::DueRelative(..) => term.allows(&values(FilterField::Due)),
        })
    }

    /// Would a task carrying exactly `value` for `field` pass this filter's terms for that field?
    pub fn field_allows(text: &str, field: FilterField, value: &str) -> bool {
        let values = [value.to_string()];
        Filter::parse(text)
            .terms
            .iter()
            .filter(|term| term.field() == Some(field))
            .all(|term| term.allows(&values))
    }

    /// The normalized spelling terms use on disk: `To Do` becomes `todo`.
    pub fn loose_key(text: &str) -> String {
        loose(text)
    }

    pub fn loose_contains(haystack: &str, needle: &str) -> bool {
        loose(haystack).contains(&loose(needle))
    }

    pub fn loose_starts_with(haystack: &str, needle: &str) -> bool {
        loose(haystack).starts_with(&loose(needle))
    }

    /// Replaces every term for `field` with `field:value`; other fields' terms stay.
    pub fn with_only(text: &str, field: FilterField, value: &str) -> String {
        let mut words = words_without_field(text, field);
        words.push(format!("{}:{}", field.keyword(), loose(value)));
        words.join(" ")
    }

    /// Rewrites `field`'s terms so exactly `shown` (out of `all`) pass, in the shortest form:
    /// no term when everything is shown, `field:!x` hides when at most half are hidden,
    /// `field:a,b` otherwise.
    pub fn with_shown(text: &str, field: FilterField, all: &[String], shown: &[String]) -> String {
        let mut words = words_without_field(text, field);
        let hidden: Vec<&String> = all.iter().filter(|value| !shown.contains(value)).collect();
        if hidden.is_empty() {
        } else if shown.is_empty() || hidden.len() <= shown.len() {
            for value in hidden {
                words.push(format!("{}:!{}", field.keyword(), loose(value)));
            }
        } else {
            let values: Vec<String> = shown.iter().map(|value| loose(value)).collect();
            words.push(format!("{}:{}", field.keyword(), values.join(",")));
        }
        words.join(" ")
    }
}

fn words_without_field(text: &str, field: FilterField) -> Vec<String> {
    text.split_whitespace()
        .filter(|word| Term::parse(word).field() != Some(field))
        .map(str::to_string)
        .collect()
}

/// "To Do", "todo", and "TO-DO" all compare equal.
fn loose(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `due:2026-09-14` is an ordinary `AnyOf` match on the raw stored date.
    #[test]
    fn due_exact_date_matches_the_stored_value() {
        assert!(Filter::field_allows(
            "due:2026-09-14",
            FilterField::Due,
            "2026-09-14"
        ));
        assert!(!Filter::field_allows(
            "due:2026-09-14",
            FilterField::Due,
            "2026-09-15"
        ));
    }

    /// `due:none` matches a task carrying no value for the field at all —
    /// the empty-values case ordinary `AnyOf` can't express.
    #[test]
    fn due_none_matches_only_the_absent_value() {
        let filter = Filter::parse("due:none");
        assert!(filter.matches_values(&[], |field| {
            assert_eq!(field, FilterField::Due);
            Vec::new()
        }));
        assert!(!filter.matches_values(&[], |_| vec!["2026-09-14".to_string()]));
    }

    #[test]
    fn due_relative_le_and_gt_split_on_the_threshold() {
        let today = crate::date_fields::today();
        let in_three_days = chrono::DateTime::from_timestamp((today + 3) * 86_400, 0)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let in_thirty_days = chrono::DateTime::from_timestamp((today + 30) * 86_400, 0)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();

        assert!(Filter::field_allows(
            "due:<=7d",
            FilterField::Due,
            &in_three_days
        ));
        assert!(!Filter::field_allows(
            "due:<=7d",
            FilterField::Due,
            &in_thirty_days
        ));
        assert!(Filter::field_allows(
            "due:>7d",
            FilterField::Due,
            &in_thirty_days
        ));
        assert!(!Filter::field_allows(
            "due:>7d",
            FilterField::Due,
            &in_three_days
        ));
    }

    /// A relative filter never matches a task with no due date at all —
    /// "within a week" cannot be true of a task with nothing to compare.
    #[test]
    fn due_relative_never_matches_a_missing_value() {
        let filter = Filter::parse("due:<=7d");
        assert!(!filter.matches_values(&[], |_| Vec::new()));
    }

    #[test]
    fn parse_due_relative_recognizes_every_comparator_and_rejects_garbage() {
        assert_eq!(
            parse_due_relative("<=7d"),
            Some(Term::DueRelative(DueCmp::Le, 7))
        );
        assert_eq!(
            parse_due_relative("<7d"),
            Some(Term::DueRelative(DueCmp::Lt, 7))
        );
        assert_eq!(
            parse_due_relative(">=7d"),
            Some(Term::DueRelative(DueCmp::Ge, 7))
        );
        assert_eq!(
            parse_due_relative(">7d"),
            Some(Term::DueRelative(DueCmp::Gt, 7))
        );
        assert_eq!(parse_due_relative("none"), None);
        assert_eq!(parse_due_relative("2026-09-14"), None);
        assert_eq!(parse_due_relative("<=7"), None, "missing the `d` suffix");
        assert_eq!(parse_due_relative("<=nd"), None, "not a number");
    }
}
