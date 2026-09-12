//! Stable UI task identity is independent of its current execution address.
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use switchbard_core::{BacklogStorageIdentity, BacklogTask};

#[derive(Debug, Clone)]
pub struct BacklogTaskKey {
    pub address: (PathBuf, String),
    pub storage_identity: Option<BacklogStorageIdentity>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Identity<'a> {
    Legacy(&'a Path, &'a str),
    Central(&'a str, &'a str),
}

impl BacklogTaskKey {
    pub fn for_task(root: &Path, task: &BacklogTask) -> Self {
        Self {
            address: (root.to_path_buf(), task.id.clone()),
            storage_identity: task.storage_identity.clone(),
        }
    }

    pub fn matches(&self, root: &Path, task: &BacklogTask) -> bool {
        match (&self.storage_identity, &task.storage_identity) {
            (Some(a), Some(b)) => a.repository_id == b.repository_id && a.record_id == b.record_id,
            (None, _) => self.address.0 == root && self.address.1 == task.id,
            _ => false,
        }
    }

    fn identity(&self) -> Identity<'_> {
        match &self.storage_identity {
            Some(identity) => Identity::Central(&identity.repository_id, &identity.record_id),
            None => Identity::Legacy(&self.address.0, &self.address.1),
        }
    }

    pub fn draft_key(&self) -> String {
        match &self.storage_identity {
            Some(identity) => format!("central:{}:{}", identity.repository_id, identity.record_id),
            None => format!("legacy:{}:{}", self.address.0.display(), self.address.1),
        }
    }
}

impl From<(PathBuf, String)> for BacklogTaskKey {
    fn from(address: (PathBuf, String)) -> Self {
        Self {
            address,
            storage_identity: None,
        }
    }
}

/// Transitional address access for command/report callsites. Equality never uses
/// this address when the central identity is present.
impl Deref for BacklogTaskKey {
    type Target = (PathBuf, String);
    fn deref(&self) -> &Self::Target {
        &self.address
    }
}
impl PartialEq for BacklogTaskKey {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}
impl Eq for BacklogTaskKey {}
impl PartialOrd for BacklogTaskKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for BacklogTaskKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.identity().cmp(&other.identity())
    }
}
impl Hash for BacklogTaskKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity().hash(state);
    }
}

pub struct TaskKeyIndex {
    stable: std::collections::HashMap<(String, String), BacklogTaskKey>,
    addresses: std::collections::HashMap<(PathBuf, String), BacklogTaskKey>,
}

impl TaskKeyIndex {
    pub fn new(repos: &std::collections::HashMap<PathBuf, switchbard_core::BacklogRepo>) -> Self {
        let mut index = Self {
            stable: Default::default(),
            addresses: Default::default(),
        };
        for (root, repo) in repos {
            for task in &repo.tasks {
                let key = BacklogTaskKey::for_task(root, task);
                if let Some(identity) = &key.storage_identity {
                    index.stable.insert(
                        (identity.repository_id.clone(), identity.record_id.clone()),
                        key.clone(),
                    );
                }
                index.addresses.insert(key.address.clone(), key);
            }
        }
        index
    }

    pub fn refresh(&self, key: BacklogTaskKey) -> BacklogTaskKey {
        let current = match &key.storage_identity {
            Some(identity) => self
                .addresses
                .get(&key.address)
                .filter(|current| current.identity() == key.identity())
                .or_else(|| {
                    self.stable
                        .get(&(identity.repository_id.clone(), identity.record_id.clone()))
                }),
            None => self.addresses.get(&key.address),
        };
        current.cloned().unwrap_or(key)
    }

    pub fn option(&self, key: &mut Option<BacklogTaskKey>) {
        *key = key.take().map(|key| self.refresh(key));
    }

    pub fn set(&self, keys: &mut std::collections::BTreeSet<BacklogTaskKey>) {
        *keys = std::mem::take(keys)
            .into_iter()
            .map(|key| self.refresh(key))
            .collect();
    }

    pub fn map<T>(&self, keys: &mut std::collections::HashMap<BacklogTaskKey, T>) {
        *keys = std::mem::take(keys)
            .into_iter()
            .map(|(key, value)| (self.refresh(key), value))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    fn key(root: &str, id: &str, revision: u64) -> BacklogTaskKey {
        BacklogTaskKey {
            address: (root.into(), id.into()),
            storage_identity: Some(BacklogStorageIdentity {
                repository_id: "repo-a".into(),
                record_id: "record-a".into(),
                revision,
            }),
        }
    }

    #[test]
    fn identity_survives_reparent_revision_and_repository_rebind() {
        let old = key("/old-repo", "TASK-1", 1);
        let current = key("/new-repo", "TASK-2.1", 5);
        assert_eq!(old, current);
        assert!(BTreeSet::from([old.clone()]).contains(&current));
        let locks = HashMap::from([(old.clone(), std::sync::Arc::new(()))]);
        assert!(locks.contains_key(&current));
        let mut other = current.clone();
        other.storage_identity.as_mut().expect("identity").record_id = "other".into();
        assert_ne!(
            old, other,
            "public addresses never override central identity"
        );
        let index = TaskKeyIndex {
            stable: HashMap::from([(("repo-a".into(), "record-a".into()), current.clone())]),
            addresses: HashMap::new(),
        };
        let old_lock = locks[&old].clone();
        let mut locks = locks;
        index.map(&mut locks);
        let refreshed = locks.keys().next().expect("key");
        assert_eq!(refreshed.address, current.address);
        assert!(std::sync::Arc::ptr_eq(&old_lock, &locks[&current]));
        assert_eq!(old.draft_key(), current.draft_key());
    }
}
