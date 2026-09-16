//! The `p` flow: what to paint, which color, and the rule hierarchy.

use crate::app::App;
use crate::columns::Column;
use crate::paint::{self, PaintRule, NAMED_COLORS};
use crate::picker::{PaintPick, PaintRuleAction, Payload, PickOption, PickerPurpose};

impl App {
    /// Mirrors the header: shown columns first (so `p2` is column 2), then the
    /// lettered targets, then hidden categorical columns by name.
    pub(super) fn open_paint_target_picker(&mut self) {
        let mut options: Vec<PickOption> = self
            .state
            .columns
            .iter()
            .map(|column| PickOption::paint_column(&self.registry, *column, false))
            .collect();
        let selected_id = if self.page == crate::page::Page::PullRequests {
            self.pull_requests.row().map(|row| row.number.to_string())
        } else {
            self.selected_task().map(|task| task.id.clone())
        };
        if let Some(id) = selected_id {
            options.push(PickOption::keyed(
                'r',
                format!("row {id}"),
                Payload::ThisRow(id),
            ));
        }
        options.extend([
            PickOption::keyed('t', "title band", Payload::PaintScope(PaintPick::Title)),
            PickOption::keyed(
                'h',
                "column headings",
                Payload::PaintScope(PaintPick::Header),
            ),
        ]);
        let has_selected_row = if self.page == crate::page::Page::PullRequests {
            self.pull_requests.row().is_some()
        } else {
            self.selected_task().is_some()
        };
        if has_selected_row {
            options.push(PickOption::keyed(
                'e',
                "selected row values",
                Payload::SelectedRowValues,
            ));
        }
        if self.page == crate::page::Page::Tasks
            && self
                .rows
                .iter()
                .any(|row| matches!(row, crate::group::Row::Heading { .. }))
        {
            options.push(PickOption::keyed(
                'g',
                "current group headings",
                Payload::GroupHeadings,
            ));
        }
        let filter = self.state.filter.trim().to_string();
        if !filter.is_empty() {
            options.push(PickOption::keyed(
                'f',
                format!("filtered rows {filter}"),
                Payload::FilteredRows(filter),
            ));
        }
        options.push(PickOption::keyed(
            'c',
            "column (whole)",
            Payload::WholeColumn,
        ));
        if !self.state.paint.is_empty() {
            options.push(PickOption::keyed(
                'o',
                format!("order rules ({})", self.state.paint.len()),
                Payload::OrderRules,
            ));
            options.push(PickOption::keyed(
                'd',
                format!("delete all paint ({} rules)", self.state.paint.len()),
                Payload::DeleteAllPaint,
            ));
        }
        for column in self.page_columns() {
            if !self.state.columns.contains(&column)
                && column.filter_field(&self.registry).is_some()
            {
                options.push(PickOption::paint_column(&self.registry, column, true));
            }
        }
        self.paint_return = None;
        self.open_picker(PickerPurpose::PaintTarget, options);
        self.telemetry.record("action", "paint");
    }

    pub(super) fn open_paint_row_values(&mut self) {
        use crate::column_values::ColumnValues;
        let mut options = Vec::new();
        for column in &self.state.columns {
            let values = if self.page == crate::page::Page::PullRequests {
                self.pull_requests
                    .row()
                    .map(|row| self.pull_requests.column_adapter(row).values(*column))
                    .unwrap_or_default()
            } else {
                self.selected_task()
                    .map(|task| {
                        column.values(&self.registry, task, &self.goals, &self.relations.blocked)
                    })
                    .unwrap_or_default()
            };
            for value in values {
                options.push(PickOption::numbered(
                    format!("{}: {value}", column.name(&self.registry)),
                    Payload::PaintScope(PaintPick::Value(*column, value)),
                ));
            }
        }
        self.open_picker(PickerPurpose::PaintRowValues, options);
    }

    pub(super) fn open_paint_headings(&mut self) {
        let mut headings = Vec::new();
        for row in self.rows.iter().take(self.selected.saturating_add(1)) {
            if let crate::group::Row::Heading { value, text, depth } = row {
                headings.truncate(*depth);
                headings.push((value.clone(), text.clone()));
            }
        }
        let options = headings
            .into_iter()
            .map(|(value, text)| {
                PickOption::numbered(text, Payload::PaintScope(PaintPick::Heading(value)))
            })
            .collect();
        self.open_picker(PickerPurpose::PaintHeadings, options);
    }

    pub(super) fn is_categorical(&self, column: Column) -> bool {
        column.filter_field(&self.registry).is_some() && !self.column_values(column).is_empty()
    }

    /// A column entry paints by its values when it has categories, else the whole column.
    pub(super) fn paint_column_entry(&mut self, column: Column) {
        if self.is_categorical(column) {
            self.open_paint_values_picker(column);
        } else {
            self.open_paint_color_picker(PaintPick::Column(column));
        }
    }

    pub(super) fn open_paint_values_picker(&mut self, column: Column) {
        let Some(_) = column.filter_field(&self.registry) else {
            return;
        };
        let mut options = vec![PickOption::numbered(
            "auto: one color per value",
            Payload::Auto,
        )];
        options.extend(
            self.column_values(column)
                .into_iter()
                .map(|(value, count)| PickOption::text(value, count)),
        );
        self.paint_return = Some(column);
        self.open_picker(PickerPurpose::PaintValues(column), options);
    }

    /// Roles first, then the theme's highlight slots as swatches (each row is
    /// drawn in its own fill and default ink), then colors and palette slots.
    /// Typing composes across all of them: `h2+alert`, `band+red`, `alert+p3`.
    pub(super) fn open_paint_color_picker(&mut self, pick: PaintPick) {
        let mut options: Vec<PickOption> = [
            ('Q', "quiet"),
            ('S', "strong"),
            ('A', "alert"),
            ('B', "band"),
            ('X', "struck"),
        ]
        .into_iter()
        .map(|(key, role)| PickOption::keyed(key, role, Payload::Text(role.into())))
        .collect();
        options.extend(
            self.config
                .theme
                .highlight_slots()
                .into_iter()
                .filter_map(crate::highlight::slot_token)
                .map(|token| PickOption::text(token, 0)),
        );
        options.extend(NAMED_COLORS.iter().map(|name| PickOption::text(*name, 0)));
        options.extend(
            (1..=self.config.palette.len()).map(|index| PickOption::text(format!("p{index}"), 0)),
        );
        options.push(PickOption::numbered("none", Payload::NoColor));
        self.open_picker(PickerPurpose::PaintColor(pick), options);
    }

    pub(super) fn open_paint_column_picker(&mut self) {
        let options = self.column_picker_options();
        self.open_picker(PickerPurpose::PaintColumn, options);
    }

    pub(super) fn open_paint_rules_picker(&mut self) {
        let options: Vec<PickOption> = self
            .state
            .paint
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                PickOption::numbered(rule.label(&self.registry), Payload::Rule(index))
            })
            .collect();
        if options.is_empty() {
            self.status = "no paint rules".to_string();
            return;
        }
        let mut options = options;
        options.extend([
            PickOption::keyed(
                'K',
                "Move a rule earlier",
                Payload::PaintRuleAction(PaintRuleAction::Earlier),
            ),
            PickOption::keyed(
                'J',
                "Move a rule later",
                Payload::PaintRuleAction(PaintRuleAction::Later),
            ),
            PickOption::keyed(
                'x',
                "Delete a rule",
                Payload::PaintRuleAction(PaintRuleAction::Delete),
            ),
        ]);
        self.open_picker(PickerPurpose::PaintRules, options);
    }

    pub(super) fn open_paint_rule_action(&mut self, action: PaintRuleAction) {
        let options = self
            .state
            .paint
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                PickOption::numbered(rule.label(&self.registry), Payload::Rule(index))
            })
            .collect();
        self.open_picker(PickerPurpose::ChoosePaintRule(action), options);
    }

    pub(super) fn run_paint_rule_action(&mut self, index: usize, action: PaintRuleAction) {
        match action {
            PaintRuleAction::Earlier => {
                self.move_paint_rule(index, -1);
            }
            PaintRuleAction::Later => {
                self.move_paint_rule(index, 1);
            }
            PaintRuleAction::Delete if index < self.state.paint.len() => {
                self.state.paint.remove(index);
                self.telemetry.record("action", "paint_rule_delete");
            }
            PaintRuleAction::Delete => {}
        }
        if self.state.paint.is_empty() {
            self.picker = None;
            self.mode = crate::app::Mode::Browse;
            self.status = "no paint rules left".to_string();
        } else {
            self.open_paint_rules_picker();
        }
    }

    pub(super) fn paint_auto(&mut self, column: Column) {
        let Some(_) = column.filter_field(&self.registry) else {
            return;
        };
        let mut rules = self.state.paint.clone();
        for (index, (value, _)) in self.column_values(column).iter().enumerate() {
            let token = format!("p{}", index + 1);
            paint::set_value_color(&mut rules, column, value, Some(&token));
        }
        if let Err(error) = paint::validate_rules(&rules) {
            self.fail(error);
            return;
        }
        self.state.paint = rules;
        self.status = format!("painted every {} value", column.name(&self.registry));
        self.telemetry.record(
            "action",
            format!("paint_auto {}", column.name(&self.registry)),
        );
    }

    pub(super) fn clear_all_paint(&mut self) {
        let count = self.state.paint.len();
        self.state.paint.clear();
        self.status = format!("deleted {count} paint rules");
        self.telemetry.record("action", "paint_clear_all");
    }

    pub(super) fn apply_paint(&mut self, pick: PaintPick, color: &str) {
        let cleared = color == "none";
        let mut rules = self.state.paint.clone();
        match &pick {
            PaintPick::Value(column, value) => {
                paint::set_value_color(&mut rules, *column, value, (!cleared).then_some(color))
            }
            PaintPick::Rows(filter) => paint::set_rule(
                &mut rules,
                PaintRule::Rows {
                    filter: filter.clone(),
                    color: color.to_string(),
                },
            ),
            PaintPick::Column(column) => paint::set_rule(
                &mut rules,
                PaintRule::Column {
                    column: *column,
                    color: color.to_string(),
                },
            ),
            PaintPick::Title => paint::set_rule(
                &mut rules,
                PaintRule::Title {
                    color: color.into(),
                },
            ),
            PaintPick::Header => paint::set_rule(
                &mut rules,
                PaintRule::Header {
                    color: color.into(),
                },
            ),
            PaintPick::Heading(value) => paint::set_rule(
                &mut rules,
                PaintRule::Heading {
                    value: value.clone(),
                    color: color.into(),
                },
            ),
        }
        if let Err(error) = paint::validate_rules(&rules) {
            self.fail(error);
            return;
        }
        self.state.paint = rules;
        self.status = if cleared {
            "paint cleared".to_string()
        } else {
            format!("painted {color}")
        };
        self.telemetry
            .record("action", format!("paint_apply {color}"));
        if let Some(column) = self.paint_return {
            self.open_paint_values_picker(column);
        }
    }

    pub(super) fn move_paint_rule(&mut self, index: usize, delta: isize) -> usize {
        let target = index as isize + delta;
        if index < self.state.paint.len()
            && target >= 0
            && (target as usize) < self.state.paint.len()
        {
            let mut rules = self.state.paint.clone();
            rules.swap(index, target as usize);
            if let Err(error) = paint::validate_rules(&rules) {
                self.fail(error);
                return index;
            }
            self.state.paint = rules;
            self.telemetry.record("action", "paint_reorder");
            return target as usize;
        }
        index
    }
}
