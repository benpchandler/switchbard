//! The `p` flow: what to paint, which color, and the rule hierarchy.

use crate::app::App;
use crate::columns::Column;
use crate::paint::{self, PaintRule, NAMED_COLORS};
use crate::picker::{PaintPick, Payload, PickOption, PickerPurpose};
use crate::tasks;

impl App {
    /// Mirrors the header: shown columns first (so `p2` is column 2), then the
    /// lettered targets, then hidden categorical columns by name.
    pub(super) fn open_paint_target_picker(&mut self) {
        let mut options: Vec<PickOption> = self
            .state
            .columns
            .iter()
            .map(|column| PickOption::column(*column, false))
            .collect();
        if let Some(task) = self.selected_task() {
            options.push(PickOption::keyed(
                'r',
                format!("row {}", task.id),
                Payload::ThisRow(task.id.clone()),
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
        for column in Column::ALL {
            if !self.state.columns.contains(&column) && column.filter_field().is_some() {
                options.push(PickOption::column(column, true));
            }
        }
        self.paint_return = None;
        self.open_picker(PickerPurpose::PaintTarget, options);
        self.telemetry.record("action", "paint");
    }

    pub(super) fn is_categorical(&self, column: Column) -> bool {
        column
            .filter_field()
            .is_some_and(|field| !tasks::field_values(&self.tasks, field).is_empty())
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
        let Some(field) = column.filter_field() else {
            return;
        };
        let mut options = vec![PickOption::numbered(
            "auto: one color per value",
            Payload::Auto,
        )];
        options.extend(
            tasks::field_values(&self.tasks, field)
                .into_iter()
                .map(|(value, count)| PickOption::text(value, count)),
        );
        self.paint_return = Some(column);
        self.open_picker(PickerPurpose::PaintValues(column), options);
    }

    pub(super) fn open_paint_color_picker(&mut self, pick: PaintPick) {
        let mut options: Vec<PickOption> = NAMED_COLORS
            .iter()
            .map(|name| PickOption::text(*name, 0))
            .collect();
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
            .map(|(index, rule)| PickOption::numbered(rule.label(), Payload::Rule(index)))
            .collect();
        if options.is_empty() {
            self.status = "no paint rules".to_string();
            return;
        }
        self.open_picker(PickerPurpose::PaintRules, options);
    }

    pub(super) fn paint_auto(&mut self, column: Column) {
        let Some(field) = column.filter_field() else {
            return;
        };
        let palette = if self.config.palette.is_empty() {
            paint::AUTO_PALETTE.map(str::to_string).to_vec()
        } else {
            self.config.palette.clone()
        };
        for (index, (value, _)) in tasks::field_values(&self.tasks, field).iter().enumerate() {
            let color = &palette[index % palette.len()];
            paint::set_value_color(&mut self.state.paint, column, value, Some(color));
        }
        self.status = format!("painted every {} value", column.name());
        self.telemetry
            .record("action", format!("paint_auto {}", column.name()));
    }

    pub(super) fn clear_all_paint(&mut self) {
        let count = self.state.paint.len();
        self.state.paint.clear();
        self.status = format!("deleted {count} paint rules");
        self.telemetry.record("action", "paint_clear_all");
    }

    pub(super) fn apply_paint(&mut self, pick: PaintPick, color: &str) {
        let cleared = color == "none";
        match &pick {
            PaintPick::Value(column, value) => paint::set_value_color(
                &mut self.state.paint,
                *column,
                value,
                (!cleared).then_some(color),
            ),
            PaintPick::Rows(filter) => paint::set_rule(
                &mut self.state.paint,
                PaintRule::Rows {
                    filter: filter.clone(),
                    color: color.to_string(),
                },
            ),
            PaintPick::Column(column) => paint::set_rule(
                &mut self.state.paint,
                PaintRule::Column {
                    column: *column,
                    color: color.to_string(),
                },
            ),
        }
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

    /// `h`/Left inside a paint flow: one level up, back to the target list.
    pub(super) fn paint_back(&mut self) {
        self.paint_return = None;
        self.open_paint_target_picker();
    }

    pub(super) fn move_paint_rule(&mut self, index: usize, delta: isize) -> usize {
        let target = index as isize + delta;
        if index < self.state.paint.len()
            && target >= 0
            && (target as usize) < self.state.paint.len()
        {
            self.state.paint.swap(index, target as usize);
            self.telemetry.record("action", "paint_reorder");
            return target as usize;
        }
        index
    }
}
