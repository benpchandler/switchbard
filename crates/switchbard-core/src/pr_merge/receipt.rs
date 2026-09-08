//! Exclusive durable intent file doubles as a cross-caller duplicate guard.
use super::{PrMergeMethod, PrMergeResult, PreparedPrMerge};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn operation_id() -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(format!(
        "{nanos}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}
pub(super) struct Receipt {
    file: File,
    pub path: PathBuf,
}
impl Receipt {
    pub fn create(
        directory: &Path,
        prepared: &PreparedPrMerge,
        method: PrMergeMethod,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let path = directory.join(format!("{}.jsonl", prepared.operation_id));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("Cannot create exclusive merge receipt: {e}"))?;
        let mut receipt = Self { file, path };
        receipt.append(&serde_json::json!({"state":"dispatch_intent","repository":prepared.repository(),
            "host":prepared.host,"number":prepared.number(),"head":prepared.head_oid(),"base":prepared.base_ref(),
            "base_oid":prepared.base_oid(),"viewer":prepared.viewer(),"method":method,"operation_id":prepared.operation_id}))?;
        File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(receipt)
    }
    fn append(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        self.file
            .write_all(&bytes)
            .and_then(|()| self.file.write_all(b"\n"))
            .and_then(|()| self.file.sync_all())
            .map_err(|e| format!("Cannot persist merge receipt: {e}"))
    }
    pub fn finish(&mut self, result: &PrMergeResult) -> Result<(), String> {
        self.append(&serde_json::json!({"state":result.outcome,"message":result.message}))
    }
}
