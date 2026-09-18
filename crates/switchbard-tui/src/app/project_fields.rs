//! Shared project attributes are edited through core, never through task patches.
use super::{App, DetailInputKind, Mode};
use crate::picker::{Payload, PickOption, PickerPurpose};
use switchbard_core::{FieldKind, ProjectDef, ProjectDefPatch};

pub(super) struct ProjectFieldDraft {
    project: ProjectDef,
    field: String,
}

impl App {
    pub fn project_field_context(&self) -> String {
        self.project_field_draft
            .as_ref()
            .map(|draft| format!("Project {} / {} (shared)", draft.project.name, draft.field))
            .unwrap_or_else(|| "Shared project value".into())
    }

    pub(super) fn open_project_fields(&mut self) {
        if self.defer_task_storage() {
            return;
        }
        let Some(name) = self.selected_task().and_then(|task| task.project.clone()) else {
            self.fail("Link a project first with t p".into());
            return;
        };
        let Some(project) = self
            .registry
            .projects
            .iter()
            .find(|project| project.name == name)
            .cloned()
        else {
            self.fail(format!(
                "Project {name} has no definition; create it with sb project create"
            ));
            return;
        };
        let fields: Vec<_> = self
            .registry
            .declared_custom()
            .filter_map(|column| {
                let spec = self.registry.spec(column);
                let name = column.name(&self.registry).strip_prefix("project.")?;
                let mut decl = spec.decl()?.clone();
                decl.name = name.to_string();
                Some(decl)
            })
            .collect();
        if fields.is_empty() {
            self.fail("No project fields declared; use sb project field add".into());
            return;
        }
        let options = fields
            .iter()
            .map(|field| {
                PickOption::numbered(
                    format!(
                        "{}: {}",
                        field.name,
                        project
                            .custom
                            .get(&field.name)
                            .map_or("Not set", String::as_str)
                    ),
                    Payload::Text(field.name.clone()),
                )
            })
            .collect();
        self.status = format!("Editing project {name}; shared by every linked task");
        self.project_field_draft = Some(ProjectFieldDraft {
            project,
            field: String::new(),
        });
        self.open_picker(PickerPurpose::ProjectFields, options);
    }

    pub(super) fn open_project_field(&mut self, field: &str) {
        let Some(draft) = self.project_field_draft.as_mut() else {
            return;
        };
        draft.field = field.to_string();
        self.input = draft.project.custom.get(field).cloned().unwrap_or_default();
        self.status = format!(
            "Project {} / {field}: shared by all linked tasks",
            draft.project.name
        );
        let declaration = self
            .registry
            .parse(&format!("project.{field}"))
            .and_then(|column| self.registry.spec(column).decl())
            .cloned();
        if let Some(decl) = declaration.filter(|decl| decl.kind == FieldKind::Enum) {
            let mut options = vec![PickOption::numbered(
                "Clear value",
                Payload::Text(String::new()),
            )];
            options.extend(
                decl.values
                    .into_iter()
                    .map(|value| PickOption::numbered(value.clone(), Payload::Text(value))),
            );
            self.open_picker(PickerPurpose::ProjectFieldValue, options);
        } else {
            self.mode = Mode::DetailInput(DetailInputKind::ProjectField);
        }
    }

    pub(super) fn save_project_field(&mut self, value: &str) {
        if self.defer_task_storage() {
            return;
        }
        let Some(draft) = self.project_field_draft.as_ref() else {
            return;
        };
        let mut patch = ProjectDefPatch::default();
        if value.is_empty() {
            patch.unset_fields.push(draft.field.clone());
        } else {
            patch
                .set_fields
                .push((draft.field.clone(), value.to_string()));
        }
        match switchbard_core::edit_project_def_if_unchanged(
            &self.repo_root,
            &draft.project.name,
            &patch,
            &draft.project,
        ) {
            Ok(_) => {
                self.status = format!(
                    "Project {} / {} saved for all linked tasks",
                    draft.project.name, draft.field
                );
                self.mode = Mode::Browse;
                self.input.clear();
                self.project_field_draft = None;
                self.reload_tasks();
            }
            Err(error) => self.fail(format!(
                "Project field not saved: {error}; reopen t f to retry"
            )),
        }
    }
}
