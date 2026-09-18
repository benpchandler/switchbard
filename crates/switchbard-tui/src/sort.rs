//! Sort orders picked with `s <column>`: plain ascending/descending, or the semantic
//! order a column's vocabulary already implies (high before low, To Do before Done).
//!
//! A view sorts by a *stack* of these, not one: layer 1 orders the list, layer 2
//! breaks its ties, and so on to [`MAX_LAYERS`]. The stack is what `s` builds as a
//! breadcrumb (`app::sort_entry`), what a saved view writes (`stack_to_text`), and
//! what every comparator walks (`compare_values`). One column appears at most once.

use std::cmp::Ordering;

use crate::columns::{Column, ColumnRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Ascending,
    Descending,
    Semantic,
}

impl Order {
    pub fn label(self, column: Column, registry: &ColumnRegistry) -> String {
        match self {
            Order::Ascending => "ascending".to_string(),
            Order::Descending => "descending".to_string(),
            Order::Semantic => match column {
                Column::Priority => "semantic (high, medium, low)".to_string(),
                Column::Status => "semantic (to do, in progress, done)".to_string(),
                // A declared enum field's order is its own `values` list, which
                // the repo can make arbitrarily long; name the column instead.
                _ => format!("semantic ({} order)", column.name(registry)),
            },
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Order::Ascending => "↑",
            Order::Descending => "↓",
            Order::Semantic => "≈",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub column: Column,
    pub order: Order,
}

impl Sort {
    pub fn label(&self, registry: &ColumnRegistry) -> String {
        format!("{}{}", self.order.glyph(), self.column.header(registry))
    }

    /// `pri:semantic`, the form saved views carry.
    pub fn to_text(&self, registry: &ColumnRegistry) -> String {
        let order = match self.order {
            Order::Ascending => "ascending",
            Order::Descending => "descending",
            Order::Semantic => "semantic",
        };
        format!("{}:{order}", self.column.save_name(registry))
    }

    pub fn parse(text: &str, registry: &ColumnRegistry) -> Option<Sort> {
        // From the right: a declared field writes itself as `field:<name>`, so
        // the column part may carry a colon of its own. An order never does.
        let (column, order) = text.trim().rsplit_once(':')?;
        let column = registry.parse(column)?;
        let order = match order {
            "ascending" => Order::Ascending,
            "descending" => Order::Descending,
            "semantic" => Order::Semantic,
            _ => return None,
        };
        Some(Sort { column, order })
    }
}

/// How many layers one sort stack may carry. Five is what a breadcrumb line reads
/// cleanly at, and deeper than that no list has distinguishable ties left.
pub const MAX_LAYERS: usize = 5;

/// What separates one layer from the next, on screen and in the breadcrumb.
pub const LAYER_SEPARATOR: &str = " \u{203a} ";

/// The stack as a breadcrumb: `\u{2191}id \u{203a} \u{2248}pri`. Empty when nothing is sorted.
pub fn stack_label(layers: &[Sort], registry: &ColumnRegistry) -> String {
    layers
        .iter()
        .map(|sort| sort.label(registry))
        .collect::<Vec<_>>()
        .join(LAYER_SEPARATOR)
}

/// The saved form: one `column:order` per layer, comma-separated. A declared
/// field writes itself as `field:<name>`, which never contains a comma.
pub fn stack_to_text(layers: &[Sort], registry: &ColumnRegistry) -> String {
    layers
        .iter()
        .map(|sort| sort.to_text(registry))
        .collect::<Vec<_>>()
        .join(",")
}

/// Reads `stack_to_text` back, and a single `column:order` from before stacks
/// existed. Unreadable layers drop; a repeated column keeps its first place;
/// anything past [`MAX_LAYERS`] is discarded rather than silently honoured.
pub fn parse_stack(text: &str, registry: &ColumnRegistry) -> Vec<Sort> {
    let mut layers: Vec<Sort> = Vec::new();
    for part in text.split(',').filter(|part| !part.trim().is_empty()) {
        let Some(sort) = Sort::parse(part, registry) else {
            continue;
        };
        if layers.iter().any(|layer| layer.column == sort.column) {
            continue;
        }
        layers.push(sort);
        if layers.len() == MAX_LAYERS {
            break;
        }
    }
    layers
}

/// Orders offered for a column; semantic only where the vocabulary has one.
pub fn orders_for(column: Column, registry: &ColumnRegistry) -> Vec<Order> {
    if column.spec(registry).vocabulary().is_empty() {
        vec![Order::Ascending, Order::Descending]
    } else {
        vec![Order::Semantic, Order::Ascending, Order::Descending]
    }
}

/// The stack, layer by layer, then the tiebreak every list shares: numeric id,
/// then identity. Deciding ties in one place is what makes a sort reproducible
/// whatever the order rows arrived in.
pub fn compare_values(
    a: &impl crate::column_values::ColumnValues,
    b: &impl crate::column_values::ColumnValues,
    layers: &[Sort],
    registry: &ColumnRegistry,
) -> Ordering {
    layers
        .iter()
        .map(|sort| compare_layer(a, b, *sort, registry))
        .find(|ordering| ordering.is_ne())
        .unwrap_or(Ordering::Equal)
        .then_with(|| tiebreak(a, b))
}

/// The shared last word on two rows a sort stack could not separate.
pub fn tiebreak(
    a: &impl crate::column_values::ColumnValues,
    b: &impl crate::column_values::ColumnValues,
) -> Ordering {
    a.numeric_key(Column::Id)
        .cmp(&b.numeric_key(Column::Id))
        .then_with(|| a.identity().cmp(b.identity()))
}

/// One layer's verdict, with no tiebreak of its own: `Equal` here is what
/// hands the decision to the next layer.
pub fn compare_layer(
    a: &impl crate::column_values::ColumnValues,
    b: &impl crate::column_values::ColumnValues,
    sort: Sort,
    registry: &ColumnRegistry,
) -> Ordering {
    // Due sorts on the raw `YYYY-MM-DD` value (lexicographic == chronological
    // for that format), with an absent due date always last regardless of
    // direction — "no due date" is not a date, so ascending/descending
    // shouldn't reposition it the way a real value would. A declared field
    // sorts on the same rule: an unset value is not a value, so it sits last
    // whichever way the rest run (`switchbard_core::custom_field_sort_key`
    // says the same for `sb list --sort`).
    if sort.column == Column::Due || matches!(sort.column, Column::Custom(_)) {
        let first = |values: &dyn crate::column_values::ColumnValues| {
            values.values(sort.column).into_iter().next()
        };
        let rank = |value: &str| sort.column.vocabulary_rank(registry, value);
        let ordering = match (first(a), first(b)) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            // Declared order for an enum field; every other kind ranks flat,
            // so the raw value is what orders it.
            (Some(a), Some(b)) => match sort.order {
                Order::Semantic => rank(&a).cmp(&rank(&b)).then_with(|| a.cmp(&b)),
                Order::Ascending => a.cmp(&b),
                Order::Descending => b.cmp(&a),
            },
        };
        return ordering;
    }
    let plain = || {
        if sort.column.spec(registry).numeric {
            a.numeric_key(sort.column).cmp(&b.numeric_key(sort.column))
        } else {
            a.values(sort.column)
                .join(",")
                .to_lowercase()
                .cmp(&b.values(sort.column).join(",").to_lowercase())
        }
    };
    let ordering = match sort.order {
        Order::Semantic => {
            let rank = |values: Vec<String>| {
                values
                    .first()
                    .map(|value| sort.column.vocabulary_rank(registry, value))
                    .unwrap_or(usize::MAX)
            };
            rank(a.values(sort.column)).cmp(&rank(b.values(sort.column)))
        }
        Order::Ascending => plain(),
        Order::Descending => plain().reverse(),
    };
    ordering
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::{BacklogTask, BacklogTaskSource};

    fn task(id: &str, due: Option<&str>) -> BacklogTask {
        BacklogTask {
            storage_identity: None,
            planning: switchbard_core::PlanningState::Considering,
            id: id.to_string(),
            title: id.to_string(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: None,
            updated_date: None,
            due_date: due.map(str::to_string),
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: std::path::PathBuf::from(format!("/repo/backlog/tasks/{id}.md")),
            custom: std::collections::BTreeMap::new(),
        }
    }

    fn sorted(tasks: &[BacklogTask], layers: &[Sort]) -> Vec<String> {
        let goals = [];
        let blocked = std::collections::HashSet::new();
        let registry = ColumnRegistry::builtin_only();
        let mut visible: Vec<usize> = (0..tasks.len()).collect();
        visible.sort_by(|&a, &b| {
            let of = |index: usize| crate::column_values::TaskValues {
                registry: &registry,
                task: &tasks[index],
                top: &[],
                goals: &goals,
                blocked: &blocked,
            };
            compare_values(&of(a), &of(b), layers, &registry)
        });
        visible.into_iter().map(|i| tasks[i].id.clone()).collect()
    }

    fn ordered(tasks: &[BacklogTask], order: Order) -> Vec<String> {
        sorted(
            tasks,
            &[Sort {
                column: Column::Due,
                order,
            }],
        )
    }

    #[test]
    fn ascending_orders_by_date_with_the_absent_value_last() {
        let tasks = [
            task("TASK-1", Some("2026-09-20")),
            task("TASK-2", None),
            task("TASK-3", Some("2026-09-14")),
        ];
        assert_eq!(
            ordered(&tasks, Order::Ascending),
            vec!["TASK-3", "TASK-1", "TASK-2"]
        );
    }

    /// Descending still keeps an absent due date last — it is not a date
    /// that reverses with the rest, it is the absence of one.
    #[test]
    fn descending_reverses_dated_tasks_but_still_keeps_the_absent_value_last() {
        let tasks = [
            task("TASK-1", Some("2026-09-20")),
            task("TASK-2", None),
            task("TASK-3", Some("2026-09-14")),
        ];
        assert_eq!(
            ordered(&tasks, Order::Descending),
            vec!["TASK-1", "TASK-3", "TASK-2"]
        );
    }

    #[test]
    fn two_absent_due_dates_break_ties_by_id() {
        let tasks = [task("TASK-2", None), task("TASK-1", None)];
        assert_eq!(ordered(&tasks, Order::Ascending), vec!["TASK-1", "TASK-2"]);
    }

    /// Layer two is what decides rows layer one calls equal; without it the
    /// shared tiebreak (numeric id) would answer instead.
    #[test]
    fn a_second_layer_breaks_the_first_layers_ties() {
        let mut tasks = [
            task("TASK-1", Some("2026-09-20")),
            task("TASK-2", Some("2026-09-20")),
            task("TASK-3", Some("2026-09-14")),
        ];
        tasks[0].title = "zebra".to_string();
        tasks[1].title = "apple".to_string();
        let layers = [
            Sort {
                column: Column::Due,
                order: Order::Ascending,
            },
            Sort {
                column: Column::Title,
                order: Order::Ascending,
            },
        ];
        assert_eq!(sorted(&tasks, &layers), ["TASK-3", "TASK-2", "TASK-1"]);
    }

    #[test]
    fn a_stack_round_trips_through_its_saved_text() {
        let registry = ColumnRegistry::builtin_only();
        let layers = [
            Sort {
                column: Column::Priority,
                order: Order::Semantic,
            },
            Sort {
                column: Column::Id,
                order: Order::Descending,
            },
        ];
        let text = stack_to_text(&layers, &registry);
        assert_eq!(text, "priority:semantic,id:descending");
        assert_eq!(parse_stack(&text, &registry), layers);
    }

    /// A view saved before stacks existed carries one `column:order` and no
    /// comma; it must still read back as a one-layer stack.
    #[test]
    fn a_single_saved_sort_reads_back_as_one_layer() {
        let registry = ColumnRegistry::builtin_only();
        assert_eq!(
            parse_stack("pri:semantic", &registry),
            [Sort {
                column: Column::Priority,
                order: Order::Semantic,
            }]
        );
    }

    #[test]
    fn a_repeated_column_keeps_its_first_place_and_the_stack_stops_at_the_limit() {
        let registry = ColumnRegistry::builtin_only();
        assert_eq!(
            parse_stack("pri:semantic,pri:ascending", &registry),
            [Sort {
                column: Column::Priority,
                order: Order::Semantic,
            }]
        );
        let every = [
            "id:ascending",
            "title:ascending",
            "status:ascending",
            "pri:ascending",
            "due:ascending",
            "labels:ascending",
        ]
        .join(",");
        assert_eq!(parse_stack(&every, &registry).len(), MAX_LAYERS);
    }
}
