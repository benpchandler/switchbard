//! The `p` flow: what to paint, which color, and the rule hierarchy.

use crate::app::App;
use crate::columns::Column;
use crate::config::Theme;
use crate::paint::{self, PaintRule, NAMED_COLORS};
use crate::picker::{PaintPick, PaintRuleAction, Payload, PickOption, PickerPurpose};

/// Tokens one drafted rule may gather, one under the rule cap so the fill
/// always fits beside them.
const MAX_DRAFT_INK: usize = crate::paint_eval::MAX_ROLE_TOKENS - 1;

/// What the two-step style picker is composing: the scope it will land on, the
/// fill chosen in step one, and the ink gathered in step two. The draft is the
/// one authority while those pickers are open, and the text it produces is the
/// same grammar `:paint` parses and a view saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintDraft {
    pub pick: PaintPick,
    pub fill: Option<String>,
    pub ink: Vec<String>,
    /// Whether the rule being edited ends in `!`. Restyling a scope must not
    /// silently unstop it: the marker belongs to the rule rather than to the
    /// roles, and losing it changes how every rule above renders.
    pub stop: bool,
    /// Whether Space has been used to gather ink in this visit. Picking a row
    /// joins what was gathered, but replaces ink that merely came from the rule
    /// the scope already wore, so restyling green to red writes `red` and not
    /// `green+red`.
    gathered: bool,
}

impl PaintDraft {
    /// Reopening a scope starts from what it already wears: the rule on it,
    /// split back into a fill and its ink by the theme that composed it.
    pub fn from_roles(
        pick: PaintPick,
        roles: Option<&str>,
        theme: &Theme,
        palette: &[String],
    ) -> PaintDraft {
        let (fill, ink) = roles
            .and_then(|roles| theme.split_roles(roles, palette))
            .unwrap_or((None, Vec::new()));
        let stop = roles.is_some_and(crate::paint_eval::stops);
        PaintDraft {
            pick,
            fill,
            ink,
            stop,
            gathered: false,
        }
    }

    /// The rule text this draft applies, stop marker included.
    pub fn roles(&self) -> String {
        crate::paint_eval::compose_text(
            self.fill.as_deref(),
            self.ink.iter().map(String::as_str),
            self.stop,
        )
    }

    /// Adds one ink token. `false` when the draft is already full, which the
    /// caller reports rather than dropping the token in silence.
    #[must_use]
    pub fn add_ink(&mut self, token: &str) -> bool {
        if self.ink.iter().any(|known| known == token) {
            return true;
        }
        if self.ink.len() >= MAX_DRAFT_INK {
            return false;
        }
        self.ink.push(token.to_string());
        true
    }

    /// Adds or removes one ink token. `false` only when adding hit the cap.
    #[must_use]
    pub fn toggle_ink(&mut self, token: &str) -> bool {
        self.gathered = true;
        match self.ink.iter().position(|known| known == token) {
            Some(index) => {
                self.ink.remove(index);
                true
            }
            None => self.add_ink(token),
        }
    }
}

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
            self.open_paint_highlight_picker(PaintPick::Column(column));
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

    /// Step one: the fill. `none`, the neutral `band`, then the theme's
    /// highlight slots as swatches, each row drawn in the fill it would apply
    /// under the ink already drafted. Picking one opens step two rather than
    /// applying, so a highlight and a text style are chosen without typing.
    pub(super) fn open_paint_highlight_picker(&mut self, pick: PaintPick) {
        let roles = self.painted_roles(&pick);
        self.paint_draft = Some(PaintDraft::from_roles(
            pick,
            roles.as_deref(),
            &self.config.theme,
            &self.config.palette,
        ));
        let mut options = vec![
            PickOption::keyed('N', "none", Payload::NoColor),
            PickOption::keyed('B', "band", Payload::Text("band".into())),
        ];
        options.extend(
            self.config
                .theme
                .highlight_slots()
                .into_iter()
                .filter_map(crate::highlight::slot_token)
                .map(|token| PickOption::text(token, 0)),
        );
        self.open_picker(PickerPurpose::PaintHighlight, options);
    }

    /// Step two: the ink over that fill. `keep default ink` leaves the cell's
    /// own text color alone; the rest are additive, so Space gathers several
    /// (`strong` and `p2`) and Enter applies what the title previews.
    pub(super) fn open_paint_text_picker(&mut self) {
        let mut options = vec![PickOption::keyed('K', "keep default ink", Payload::NoColor)];
        options.extend(
            [
                ('Q', "quiet"),
                ('S', "strong"),
                ('A', "alert"),
                ('X', "struck"),
            ]
            .into_iter()
            .map(|(key, role)| PickOption::keyed(key, role, Payload::Text(role.into()))),
        );
        options.extend(NAMED_COLORS.iter().map(|name| PickOption::text(*name, 0)));
        options.extend(
            (1..=self.config.palette.len()).map(|index| PickOption::text(format!("p{index}"), 0)),
        );
        self.open_picker(PickerPurpose::PaintText, options);
    }

    /// The rule this scope already wears, so reopening the picker starts from
    /// what is on screen instead of from nothing.
    fn painted_roles(&self, pick: &PaintPick) -> Option<String> {
        let rules = &self.state.paint;
        match pick {
            PaintPick::Value(column, value) => paint::value_color(rules, *column, value),
            PaintPick::Rows(filter) => rules.iter().find_map(|rule| match rule {
                PaintRule::Rows {
                    filter: known,
                    color,
                } if known == filter => Some(color.clone()),
                _ => None,
            }),
            PaintPick::Column(column) => rules.iter().find_map(|rule| match rule {
                PaintRule::Column {
                    column: known,
                    color,
                } if known == column => Some(color.clone()),
                _ => None,
            }),
            PaintPick::Heading(value) => rules.iter().find_map(|rule| match rule {
                PaintRule::Heading {
                    value: known,
                    color,
                } if known == value => Some(color.clone()),
                _ => None,
            }),
            PaintPick::Header => rules.iter().find_map(|rule| match rule {
                PaintRule::Header { color } => Some(color.clone()),
                _ => None,
            }),
            PaintPick::Title => rules.iter().find_map(|rule| match rule {
                PaintRule::Title { color } => Some(color.clone()),
                _ => None,
            }),
        }
    }

    /// Step one picked: remember the fill and move to the ink.
    pub(super) fn choose_paint_highlight(&mut self, fill: Option<&str>) {
        if let Some(draft) = self.paint_draft.as_mut() {
            draft.fill = fill.map(str::to_string);
        }
        self.open_paint_text_picker();
    }

    /// `keep default ink` with Space: drop every token gathered so far.
    pub(super) fn clear_paint_ink(&mut self) {
        if let Some(draft) = self.paint_draft.as_mut() {
            draft.ink.clear();
        }
    }

    /// Space in step two: add or remove one ink token, leaving the picker open
    /// so the title shows the composition growing.
    pub(super) fn toggle_paint_ink(&mut self, token: &str) {
        let added = self
            .paint_draft
            .as_mut()
            .is_none_or(|draft| draft.toggle_ink(token));
        if !added {
            self.status = format!("ink limit reached: {MAX_DRAFT_INK} text tokens");
        }
    }

    /// Enter, a number or a letter in step two: add the row that was picked,
    /// then apply everything drafted. Adding is idempotent, so picking a row
    /// already gathered with Space just applies.
    pub(super) fn apply_paint_draft(&mut self, ink: Option<&str>) {
        let Some(mut draft) = self.paint_draft.take() else {
            return;
        };
        let added = match ink {
            Some(token) => {
                if !draft.gathered {
                    draft.ink.clear();
                }
                draft.add_ink(token)
            }
            // `keep default ink` is the absence of ink, including the ink the
            // scope already wore when the picker opened.
            None => {
                draft.ink.clear();
                true
            }
        };
        let roles = draft.roles();
        let roles = if roles.is_empty() {
            "none"
        } else {
            roles.as_str()
        };
        self.apply_paint(draft.pick, roles);
        if !added {
            self.status = format!(
                "{} · ink limit reached: {MAX_DRAFT_INK} text tokens",
                self.status
            );
        }
    }

    /// A rule typed in full at either step replaces the whole composition.
    pub(super) fn apply_typed_paint(&mut self, roles: &str) {
        let Some(draft) = self.paint_draft.take() else {
            return;
        };
        self.apply_paint(draft.pick, roles);
    }

    /// What the title previews: what Enter would produce right now. A rule
    /// typed in full lands whole, so it previews alone; otherwise the row under
    /// the cursor is previewed composed with the rest of the draft, which is
    /// what picking it would apply.
    pub fn paint_preview(&self, picker: &crate::picker::ValuePicker) -> Option<String> {
        let typed = picker.typed.trim();
        let highlighted = picker.highlighted();
        if highlighted.is_none() && !typed.is_empty() && paint::validate_roles(typed).is_ok() {
            return Some(typed.to_string());
        }
        let token = match highlighted.as_ref().map(|option| &option.payload) {
            Some(Payload::Text(token)) => Some(token.clone()),
            _ => None,
        };
        self.paint_row_preview(&picker.purpose, token.as_deref())
    }

    /// How one row of either step would look if it were picked: the row's own
    /// token composed with the rest of the draft.
    pub fn paint_row_preview(
        &self,
        purpose: &PickerPurpose,
        token: Option<&str>,
    ) -> Option<String> {
        let draft = self.paint_draft.as_ref()?;
        let drafted = draft.ink.iter().map(String::as_str);
        let roles = match purpose {
            PickerPurpose::PaintHighlight => {
                crate::paint_eval::compose_text(token, drafted, draft.stop)
            }
            PickerPurpose::PaintText => {
                // The `keep default ink` row previews the fill alone; every
                // other row previews itself joining what is already drafted.
                let (drafted, extra) = match token {
                    Some(token) => (
                        Some(drafted),
                        (!draft.ink.iter().any(|known| known == token)).then_some(token),
                    ),
                    None => (None, None),
                };
                crate::paint_eval::compose_text(
                    draft.fill.as_deref(),
                    drafted.into_iter().flatten().chain(extra),
                    draft.stop,
                )
            }
            _ => return None,
        };
        (!roles.is_empty()).then_some(roles)
    }

    /// Whether the row for `token` is part of the draft, which is the mark the
    /// picker shows beside it.
    pub fn paint_draft_holds(&self, purpose: &PickerPurpose, token: Option<&str>) -> bool {
        let Some(draft) = self.paint_draft.as_ref() else {
            return false;
        };
        match (purpose, token) {
            (PickerPurpose::PaintHighlight, Some(token)) => draft.fill.as_deref() == Some(token),
            (PickerPurpose::PaintHighlight, None) => draft.fill.is_none(),
            (PickerPurpose::PaintText, Some(token)) => draft.ink.iter().any(|known| known == token),
            (PickerPurpose::PaintText, None) => draft.ink.is_empty(),
            _ => false,
        }
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
        // A finished style is not a step to go back to: drop both of its
        // pickers from the back stack so `←` from whatever reopens next
        // returns to the scope list.
        self.picker_parents.retain(|parent| {
            !matches!(
                parent.purpose,
                PickerPurpose::PaintHighlight | PickerPurpose::PaintText
            )
        });
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
