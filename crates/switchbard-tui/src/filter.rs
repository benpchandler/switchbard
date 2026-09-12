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
    Blocked,
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
            "blocked" => FilterField::Blocked,
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
            FilterField::Blocked => "blocked",
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
            FilterField::Blocked => Column::Blocked,
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

/// One word of a filter. `pri:high,medium` is one term matching either value;
/// `status:!done` hides a value; a bare word searches id and title.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Text(String),
    AnyOf(FilterField, Vec<String>),
    Not(FilterField, String),
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
        match value.strip_prefix('!') {
            Some(hidden) => Term::Not(field, boolean_alias(field, loose(hidden))),
            None => Term::AnyOf(
                field,
                value
                    .split(',')
                    .map(|v| boolean_alias(field, loose(v)))
                    .collect(),
            ),
        }
    }

    fn field(&self) -> Option<FilterField> {
        match self {
            Term::Text(_) => None,
            Term::AnyOf(field, _) | Term::Not(field, _) => Some(*field),
        }
    }

    fn allows(&self, values: &[String]) -> bool {
        match self {
            Term::Text(_) => true,
            // `id:` is exact so a painted TASK-13 never also paints TASK-130.
            Term::AnyOf(FilterField::Id | FilterField::Tasks, wanted) => values
                .iter()
                .any(|value| wanted.iter().any(|want| loose(value) == *want)),
            Term::AnyOf(_, wanted) => values
                .iter()
                .any(|value| wanted.iter().any(|want| loose(value).contains(want))),
            Term::Not(_, hidden) => !values.iter().any(|value| loose(value) == *hidden),
        }
    }
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

/// `blocked:true|false` reads as `blocked:yes|no` — the column's own values
/// (task-209.2's `blocked:yes|no`) stay the one spelling the picker, glyph
/// legend, and paint rules all show; this only widens what the typed filter
/// itself accepts, the same way `status:!done` is Backlog.md's spelling but
/// `!` is generic filter grammar.
fn boolean_alias(field: FilterField, value: String) -> String {
    if field != FilterField::Blocked {
        return value;
    }
    match value.as_str() {
        "true" => "yes".to_string(),
        "false" => "no".to_string(),
        _ => value,
    }
}

/// "To Do", "todo", and "TO-DO" all compare equal.
fn loose(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}
