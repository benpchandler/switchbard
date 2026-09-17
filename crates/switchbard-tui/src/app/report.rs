//! A single owned report submission; storage work never runs on the input thread.
use super::App;
use crate::report::{file_report, ReportContext, ReportKind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

struct ReportRequest {
    target: PathBuf,
    local: bool,
    kind: ReportKind,
    intent: String,
    location: String,
    screen: String,
    trail: Vec<String>,
}

impl ReportRequest {
    fn spawn(self) -> std::io::Result<Receiver<Result<String, String>>> {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("sbt-report".into())
            .spawn(move || {
                let result = file_report(
                    &self.target,
                    self.kind,
                    ReportContext {
                        intent: &self.intent,
                        location: &self.location,
                        screen: &self.screen,
                        trail: &self.trail,
                    },
                )
                .map(|id| {
                    if self.local {
                        id
                    } else {
                        format!("{id} in {}", self.target.display())
                    }
                })
                .map_err(|error| error.to_string());
                if tx.send(result).is_err() { /* app terminated externally */ }
            })?;
        Ok(rx)
    }
}

#[derive(Default)]
pub struct ReportFlow {
    pending: Option<Receiver<Result<String, String>>>,
    message: Option<String>,
    select_after: Option<(String, u64)>,
    retry_command: Option<String>,
    kind: Option<ReportKind>,
    location: String,
    selected_identity: Option<switchbard_core::BacklogStorageIdentity>,
}

impl ReportFlow {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub(super) fn retry_command(&self) -> Option<String> {
        if self.is_pending() {
            None
        } else {
            self.retry_command.clone()
        }
    }
    pub fn dismiss(&mut self) -> bool {
        if self.pending.is_some() {
            return true;
        }
        self.message.take().is_some()
    }
}

impl App {
    pub(super) fn file_report(&mut self, kind: ReportKind, intent: &str) {
        if self.report.is_pending() {
            self.status = "Saving report; draft retained, wait before submitting another".into();
            self.input = format!("{} {intent}", kind.label());
            self.mode = super::Mode::Command;
            return;
        }
        if intent.trim().is_empty() {
            self.fail("say what you were trying to do: :bug <text> or :idea <text>".into());
            return;
        }
        self.start_report(kind, intent);
    }

    fn start_report(&mut self, kind: ReportKind, intent: &str) {
        let target = self
            .config
            .report_repo
            .clone()
            .unwrap_or_else(|| self.repo_root.clone());
        let local = target == self.repo_root;
        let request = ReportRequest {
            target,
            local,
            kind,
            intent: intent.to_string(),
            location: self.location(),
            screen: self.last_screen.clone(),
            trail: self.telemetry.trail(),
        };
        self.report.location = request.location.clone();
        self.report.selected_identity = self
            .selected_task()
            .and_then(|task| task.storage_identity.clone());
        self.report.kind = Some(kind);
        self.report.retry_command = Some(format!("{} {intent}", kind.label()));
        self.report.select_after = None;
        match request.spawn() {
            Ok(rx) => {
                self.report.pending = Some(rx);
                self.report.message = Some("Saving report... navigation remains available".into());
                self.report.select_after =
                    local.then(|| (String::new(), self.interaction_generation));
            }
            Err(error) => {
                self.report.message =
                    Some(format!("Report not started; : restores draft - {error}"))
            }
        }
    }

    pub(super) fn tick_report(&mut self) {
        let Some(rx) = &self.report.pending else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.report.pending = None;
                self.report.select_after = None;
                self.publish_report(
                    "Report outcome unknown; check backlog before retrying (: restores draft)"
                        .into(),
                    true,
                );
                return;
            }
        };
        self.report.pending = None;
        self.accept_report_result(result);
    }

    fn accept_report_result(&mut self, result: Result<String, String>) {
        let failed = result.is_err();
        let message = match result {
            Ok(id) => {
                if let Some((selected, _)) = &mut self.report.select_after {
                    *selected = id.clone();
                }
                self.report.retry_command = None;
                if self.report.select_after.is_some() {
                    self.task_generation = self.task_generation.wrapping_add(1);
                    self.request_task_refresh();
                }
                format!("filed {id}")
            }
            Err(error) => {
                self.report.select_after = None;
                format!("Report failed; : restores draft - {error}")
            }
        };
        self.publish_report(message, failed);
    }

    fn publish_report(&mut self, message: String, failed: bool) {
        let kind = self
            .report
            .kind
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_default();
        self.telemetry.record(
            if failed { "error" } else { "report" },
            format!("{kind} {message}"),
        );
        self.status = message.clone();
        self.report.message = Some(message);
    }

    pub(super) fn select_filed_report(&mut self) {
        if self.report.is_pending() {
            return;
        }
        let Some((id, generation)) = self.report.select_after.take() else {
            return;
        };
        let filed = self
            .tasks
            .iter()
            .position(|task| task.id.rsplit('-').next() == Some(id.as_str()));
        if let Some(index) = filed {
            let message = format!("filed {}", self.tasks[index].id);
            if self.status == format!("filed {id}") {
                self.status = message.clone();
            }
            if self.report.message.is_some() {
                self.report.message = Some(message);
            }
        }
        if id.is_empty()
            || generation != self.interaction_generation
            || self.location() != self.report.location
            || self
                .selected_task()
                .and_then(|task| task.storage_identity.as_ref())
                != self.report.selected_identity.as_ref()
            || self.page != crate::page::Page::Tasks
        {
            return;
        }
        if let Some(row) = filed.and_then(|index| {
            self.rows
                .iter()
                .position(|row| *row == crate::group::Row::Task(index))
        }) {
            self.select(row);
        }
    }
}
