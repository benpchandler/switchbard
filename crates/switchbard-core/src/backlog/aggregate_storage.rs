//! Explicit command-scoped snapshot for the legacy YAML aggregate cutover.
//! Commands stage raw text and commit once, rejecting concurrent edits even on no-ops.
use crate::storage::{Document, RepositoryId, RepositoryLock, Store};
use anyhow::{ensure, Context, Result};
use std::path::Path;

struct CentralEdit {
    _repository_lock: RepositoryLock,
    store: Store,
    repo: RepositoryId,
    original: Option<Document>,
    draft: Option<Vec<u8>>,
}

pub(super) struct AggregateEdit {
    _repository_lock: Option<RepositoryLock>,
    central: Option<CentralEdit>,
    detached: Option<Option<Vec<u8>>>,
    kind: &'static str,
    locator: &'static str,
}

pub(super) fn read(
    root: &Path,
    kind: &'static str,
    locator: &'static str,
) -> Result<Option<Option<String>>> {
    let Some(store) = Store::open_existing_default()? else {
        return Ok(None);
    };
    let Some(repo) = store.authority_for_root(root, kind)? else {
        return Ok(None);
    };
    let Some(document) = store.read(&repo, kind, locator)? else {
        return Ok(Some(None));
    };
    document.ensure_understood()?;
    if document.deleted {
        return Ok(Some(None));
    }
    Ok(Some(Some(String::from_utf8(document.content).context("stored aggregate is not UTF-8")?)))
}

pub(super) fn with_edit<T>(
    root: &Path,
    kind: &'static str,
    locator: &'static str,
    operation: impl FnOnce(&mut AggregateEdit) -> Result<T>,
) -> Result<T> {
    let mut edit = AggregateEdit::begin(root, kind, locator)?;
    let result = operation(&mut edit)?;
    edit.finish()?;
    Ok(result)
}

impl AggregateEdit {
    pub(super) fn from_document(content: Option<&[u8]>) -> Self {
        Self {
            _repository_lock: None,
            central: None,
            detached: Some(content.map(<[u8]>::to_vec)),
            kind: "",
            locator: "",
        }
    }

    pub(super) fn into_document(self) -> Option<Vec<u8>> {
        self.detached.expect("detached document draft")
    }

    fn begin(root: &Path, kind: &'static str, locator: &'static str) -> Result<Self> {
        let mut repository_lock = Some(RepositoryLock::acquire(root)?);
        let central = if let Some(store) = Store::open_existing_default()? {
            if let Some(repo) = store.authority_for_root(root, kind)? {
                let original = store.read(&repo, kind, locator)?;
                if let Some(document) = &original {
                    document.ensure_understood()?;
                }
                let draft = original
                    .as_ref()
                    .filter(|doc| !doc.deleted)
                    .map(|doc| doc.content.clone());
                Some(CentralEdit {
                    _repository_lock: repository_lock.take().expect("repository lock present"),
                    store,
                    repo,
                    original,
                    draft,
                })
            } else {
                None
            }
        } else {
            None
        };
        Ok(Self {
            _repository_lock: repository_lock,
            central,
            detached: None,
            kind,
            locator,
        })
    }

    pub(super) fn is_central(&self) -> bool {
        self.central.is_some() || self.detached.is_some()
    }

    pub(super) fn exists(&self, path: &Path) -> bool {
        if let Some(draft) = &self.detached {
            return draft.is_some();
        }
        self.central
            .as_ref()
            .map_or_else(|| path.is_file(), |state| state.draft.is_some())
    }

    fn central_text(&self) -> Result<Option<Option<String>>> {
        if let Some(draft) = &self.detached {
            return Ok(Some(
                draft
                    .as_ref()
                    .map(|bytes| String::from_utf8(bytes.clone()).context("aggregate is not UTF-8"))
                    .transpose()?,
            ));
        }
        self.central
            .as_ref()
            .map(|state| {
                state
                    .draft
                    .as_ref()
                    .map(|bytes| {
                        String::from_utf8(bytes.clone()).context("stored aggregate is not UTF-8")
                    })
                    .transpose()
            })
            .transpose()
    }

    pub(super) fn text(&self, path: &Path) -> Result<String> {
        match self.central_text()? {
            Some(text) => text.context("aggregate is not yet defined"),
            None => {
                std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
            }
        }
    }

    /// Returns true when staged centrally, false when the legacy writer should proceed.
    pub(super) fn stage(&mut self, text: &str) -> bool {
        if let Some(draft) = &mut self.detached {
            *draft = Some(text.as_bytes().to_vec());
            return true;
        }
        if let Some(state) = &mut self.central {
            state.draft = Some(text.as_bytes().to_vec());
            true
        } else {
            false
        }
    }

    fn finish(self) -> Result<()> {
        let Some(mut state) = self.central else {
            return Ok(());
        };
        let expected = state.original.as_ref().map(|doc| doc.revision);
        state
            .store
            .mutate(&state.repo, self.kind, self.locator, expected, |current| {
                ensure!(
                    expected.is_some() || current.is_none(),
                    "aggregate created concurrently; reload and retry"
                );
                Ok(state.draft)
            })?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "aggregate_storage_tests.rs"]
mod tests;
