//! Verified one-request process boundary for the bundled xplan Mission helper.

use crate::mission_sidecar_protocol::{
    MissionRequest, MissionResponse, MAX_MISSION_REQUEST_BYTES, MISSION_SIDECAR_PROTOCOL,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const TERMINATION_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct MissionSupervisorConfig {
    pub executable_root: PathBuf,
    pub helper_path: PathBuf,
    pub manifest_path: PathBuf,
    pub state_root: PathBuf,
    pub timeout: Duration,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
}

#[derive(Debug, Clone)]
pub struct MissionSupervisor {
    config: MissionSupervisorConfig,
    helper: PathBuf,
    helper_digest: String,
    manifest: Option<ArtifactManifest>,
    last_reaped: Arc<AtomicBool>,
    retry_sequence: Arc<AtomicU64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionSupervisorError {
    Manifest(String),
    Input(String),
    Io(String),
    Timeout,
    OutputLimit(&'static str),
    Protocol(String),
    Process(i32),
    Remote(String),
}

impl MissionSupervisorError {
    #[must_use]
    pub fn is_manifest_rejection(&self) -> bool {
        matches!(self, Self::Manifest(_))
    }

    #[must_use]
    pub fn is_protocol_rejection(&self) -> bool {
        matches!(self, Self::Protocol(_))
    }

    #[must_use]
    pub fn is_bounded_failure(&self) -> bool {
        !matches!(self, Self::Manifest(_) | Self::Input(_))
    }

    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        matches!(
            self,
            Self::Io(_)
                | Self::Timeout
                | Self::OutputLimit(_)
                | Self::Protocol(_)
                | Self::Process(_)
        )
    }

    #[must_use]
    pub fn remote_code(&self) -> Option<&str> {
        match self {
            Self::Remote(code) => Some(code),
            _ => None,
        }
    }
}

impl std::fmt::Display for MissionSupervisorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(message) => write!(formatter, "helper manifest rejected: {message}"),
            Self::Input(message) => write!(formatter, "mission request rejected: {message}"),
            Self::Io(message) => write!(formatter, "helper transport unavailable: {message}"),
            Self::Timeout => write!(formatter, "helper timed out before acknowledgement"),
            Self::OutputLimit(stream) => write!(formatter, "helper {stream} exceeded its limit"),
            Self::Protocol(message) => write!(formatter, "helper protocol rejected: {message}"),
            Self::Process(code) => {
                write!(formatter, "helper exited without acknowledgement ({code})")
            }
            Self::Remote(code) => write!(formatter, "xplan rejected the mission command ({code})"),
        }
    }
}

impl std::error::Error for MissionSupervisorError {}

impl MissionSupervisor {
    pub fn new(config: MissionSupervisorConfig) -> Result<Self, MissionSupervisorError> {
        validate_config(&config)?;
        let manifest = load_manifest(&config.manifest_path)?;
        let root = config
            .manifest_path
            .parent()
            .ok_or_else(|| manifest_error("manifest has no parent"))?;
        verify_manifest(&manifest, root)?;
        let helper = resolve_helper(&config)?;
        let helper_digest = sha256_file(&helper).map_err(io_error)?;
        Ok(Self::from_parts(
            config,
            helper,
            helper_digest,
            Some(manifest),
        ))
    }

    pub fn from_verified_helper(
        helper: impl AsRef<Path>,
        state_root: impl AsRef<Path>,
    ) -> Result<Self, MissionSupervisorError> {
        Self::from_direct_helper(
            helper.as_ref(),
            state_root.as_ref(),
            Duration::from_secs(20),
        )
    }

    pub fn from_test_fixture(
        helper: impl AsRef<Path>,
        state_root: impl AsRef<Path>,
    ) -> Result<Self, MissionSupervisorError> {
        Self::from_direct_helper(helper.as_ref(), state_root.as_ref(), Duration::from_secs(5))
    }

    fn from_direct_helper(
        helper: &Path,
        state_root: &Path,
        timeout: Duration,
    ) -> Result<Self, MissionSupervisorError> {
        let helper = fs::canonicalize(helper).map_err(io_error)?;
        require_executable(&helper)?;
        let executable_root = helper
            .parent()
            .ok_or_else(|| manifest_error("helper has no parent"))?
            .to_path_buf();
        let config = MissionSupervisorConfig {
            executable_root,
            helper_path: PathBuf::from(
                helper
                    .file_name()
                    .ok_or_else(|| manifest_error("helper has no filename"))?,
            ),
            manifest_path: PathBuf::new(),
            state_root: state_root.to_path_buf(),
            timeout,
            stdout_limit: 1_048_576,
            stderr_limit: 65_536,
        };
        let digest = sha256_file(&helper).map_err(io_error)?;
        Ok(Self::from_parts(config, helper, digest, None))
    }

    fn from_parts(
        config: MissionSupervisorConfig,
        helper: PathBuf,
        helper_digest: String,
        manifest: Option<ArtifactManifest>,
    ) -> Self {
        Self {
            config,
            helper,
            helper_digest,
            manifest,
            last_reaped: Arc::new(AtomicBool::new(false)),
            retry_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn invoke(
        &self,
        request: MissionRequest,
    ) -> Result<MissionResponse, MissionSupervisorError> {
        self.verify_current_artifact()?;
        let input = encode_request(&request)?;
        self.last_reaped.store(false, Ordering::SeqCst);
        let mut child = self.spawn()?;
        let pid = child.id() as i32;
        let writer = spawn_writer(child.stdin.take(), input);
        let stdout = spawn_reader(child.stdout.take(), self.config.stdout_limit);
        let stderr = spawn_reader(child.stderr.take(), self.config.stderr_limit);
        let status = wait_bounded(&mut child, pid, self.config.timeout);
        self.last_reaped.store(true, Ordering::SeqCst);
        let _ = writer.join();
        let stdout = join_reader(stdout)?;
        let stderr = join_reader(stderr)?;
        classify_output(status?, stdout, stderr, &request)
    }

    pub fn prepare_retry(
        &self,
        request: &MissionRequest,
        error: &MissionSupervisorError,
    ) -> Result<MissionRequest, MissionSupervisorError> {
        if !error.is_ambiguous() {
            return Err(MissionSupervisorError::Input(
                "only an ambiguous transport outcome may be retried".to_owned(),
            ));
        }
        let sequence = self.retry_sequence.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(request.retry_with_request_id(format!("retry-{sequence}-{}", request.command_id)))
    }

    #[must_use]
    pub fn last_process_group_reaped(&self) -> bool {
        self.last_reaped.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.config.state_root
    }

    pub fn build_test_manifest(helper: &Path) -> Result<Vec<u8>, MissionSupervisorError> {
        let name = helper
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| manifest_error("test helper has no safe filename"))?;
        let entry = ManifestEntry {
            path: name.to_owned(),
            kind: ManifestKind::File,
            size: Some(fs::metadata(helper).map_err(io_error)?.len()),
            sha256: Some(sha256_file(helper).map_err(io_error)?),
            link_target: None,
        };
        let manifest = ArtifactManifest {
            schema_version: 1,
            source_revision: "0".repeat(40),
            uv_lock_sha256: "0".repeat(64),
            version: "test".to_owned(),
            target_os: current_target_os().to_owned(),
            arch: current_arch().to_owned(),
            dependencies: Value::Object(Default::default()),
            files: vec![entry],
        };
        serde_json::to_vec(&manifest).map_err(|error| manifest_error(error.to_string()))
    }

    fn spawn(&self) -> Result<Child, MissionSupervisorError> {
        let mut command = Command::new(&self.helper);
        command
            .arg("--state-root")
            .arg(&self.config.state_root)
            .env_clear()
            .env("XPLAN_MISSION_STATE", &self.config.state_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        copy_allowed_environment(&mut command);
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command.spawn().map_err(io_error)
    }

    fn verify_current_artifact(&self) -> Result<(), MissionSupervisorError> {
        if let Some(manifest) = &self.manifest {
            let root = self
                .config
                .manifest_path
                .parent()
                .ok_or_else(|| manifest_error("manifest has no parent"))?;
            verify_manifest(manifest, root)?;
        }
        let current = sha256_file(&self.helper).map_err(io_error)?;
        if current != self.helper_digest {
            return Err(manifest_error("helper digest changed after verification"));
        }
        Ok(())
    }
}

fn validate_config(config: &MissionSupervisorConfig) -> Result<(), MissionSupervisorError> {
    if !safe_relative(&config.helper_path) {
        return Err(manifest_error("helper path must be safe and relative"));
    }
    if config.timeout.is_zero() || config.stdout_limit == 0 || config.stderr_limit == 0 {
        return Err(MissionSupervisorError::Input(
            "supervisor bounds must be positive".to_owned(),
        ));
    }
    Ok(())
}

fn resolve_helper(config: &MissionSupervisorConfig) -> Result<PathBuf, MissionSupervisorError> {
    let root = fs::canonicalize(&config.executable_root).map_err(io_error)?;
    let helper = fs::canonicalize(root.join(&config.helper_path)).map_err(io_error)?;
    if !helper.starts_with(&root) {
        return Err(manifest_error("helper escapes executable root"));
    }
    require_executable(&helper)?;
    Ok(helper)
}

fn require_executable(path: &Path) -> Result<(), MissionSupervisorError> {
    let metadata = fs::metadata(path).map_err(io_error)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(manifest_error("helper is absent or non-executable"));
    }
    Ok(())
}

fn encode_request(request: &MissionRequest) -> Result<Vec<u8>, MissionSupervisorError> {
    if request.protocol_version != MISSION_SIDECAR_PROTOCOL {
        return Err(MissionSupervisorError::Input(
            "unsupported request protocol".to_owned(),
        ));
    }
    let pretty = serde_json::to_string_pretty(request)
        .map_err(|error| MissionSupervisorError::Input(error.to_string()))?;
    let mut input = pretty
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
        .into_bytes();
    input.push(b'\n');
    if input.len() > MAX_MISSION_REQUEST_BYTES {
        return Err(MissionSupervisorError::Input(
            "mission request exceeds 1 MiB".to_owned(),
        ));
    }
    Ok(input)
}

fn copy_allowed_environment(command: &mut Command) {
    for name in ["HOME", "PATH", "LANG", "LC_ALL", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

fn spawn_writer(
    stdin: Option<std::process::ChildStdin>,
    input: Vec<u8>,
) -> thread::JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut stdin = stdin.ok_or_else(|| io::Error::other("helper stdin unavailable"))?;
        stdin.write_all(&input)?;
        stdin.flush()
    })
}

fn spawn_reader<R: Read + Send + 'static>(
    stream: Option<R>,
    limit: usize,
) -> thread::JoinHandle<io::Result<BoundedOutput>> {
    thread::spawn(move || {
        let mut stream = stream.ok_or_else(|| io::Error::other("helper stream unavailable"))?;
        read_bounded(&mut stream, limit)
    })
}

fn read_bounded(stream: &mut impl Read, limit: usize) -> io::Result<BoundedOutput> {
    let mut stored = Vec::with_capacity(limit.min(64 * 1024));
    let mut total = 0usize;
    let mut buffer = [0u8; 8192];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count);
        if stored.len() < limit {
            let keep = count.min(limit - stored.len());
            stored.extend_from_slice(&buffer[..keep]);
        }
    }
    Ok(BoundedOutput {
        bytes: stored,
        exceeded: total > limit,
    })
}

fn wait_bounded(
    child: &mut Child,
    pgid: i32,
    timeout: Duration,
) -> Result<std::process::ExitStatus, MissionSupervisorError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(io_error)? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            terminate_and_reap(child, pgid)?;
            return Err(MissionSupervisorError::Timeout);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn terminate_and_reap(child: &mut Child, pgid: i32) -> Result<(), MissionSupervisorError> {
    let _ = unsafe { libc::kill(-pgid, libc::SIGTERM) };
    let deadline = Instant::now() + TERMINATION_GRACE;
    while Instant::now() < deadline {
        if child.try_wait().map_err(io_error)?.is_some() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    child.wait().map_err(io_error)?;
    Ok(())
}

fn join_reader(
    reader: thread::JoinHandle<io::Result<BoundedOutput>>,
) -> Result<BoundedOutput, MissionSupervisorError> {
    reader
        .join()
        .map_err(|_| MissionSupervisorError::Io("helper reader panicked".to_owned()))?
        .map_err(io_error)
}

fn classify_output(
    status: std::process::ExitStatus,
    stdout: BoundedOutput,
    stderr: BoundedOutput,
    request: &MissionRequest,
) -> Result<MissionResponse, MissionSupervisorError> {
    if stdout.exceeded {
        return Err(MissionSupervisorError::OutputLimit("stdout"));
    }
    if stderr.exceeded {
        return Err(MissionSupervisorError::OutputLimit("stderr"));
    }
    let line = single_response(&stdout.bytes)?;
    let response = MissionResponse::decode(line).map_err(MissionSupervisorError::Protocol)?;
    response
        .validate_identity(request)
        .map_err(MissionSupervisorError::Protocol)?;
    if let Some(code) = response.remote_error_code() {
        return Err(MissionSupervisorError::Remote(code.to_owned()));
    }
    if !status.success() {
        return Err(MissionSupervisorError::Process(status.code().unwrap_or(-1)));
    }
    Ok(response)
}

fn single_response(bytes: &[u8]) -> Result<&[u8], MissionSupervisorError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| MissionSupervisorError::Protocol("response is not UTF-8".to_owned()))?;
    let lines: Vec<_> = value.lines().collect();
    if lines.len() != 1 || lines[0].trim().is_empty() {
        return Err(MissionSupervisorError::Protocol(
            "helper must emit exactly one response".to_owned(),
        ));
    }
    Ok(lines[0].as_bytes())
}

#[derive(Debug)]
struct BoundedOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactManifest {
    schema_version: u64,
    source_revision: String,
    uv_lock_sha256: String,
    version: String,
    target_os: String,
    arch: String,
    dependencies: Value,
    files: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    path: String,
    kind: ManifestKind,
    size: Option<u64>,
    sha256: Option<String>,
    link_target: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ManifestKind {
    File,
    Symlink,
}

fn load_manifest(path: &Path) -> Result<ArtifactManifest, MissionSupervisorError> {
    let bytes = fs::read(path).map_err(io_error)?;
    serde_json::from_slice(&bytes).map_err(|error| manifest_error(error.to_string()))
}

fn verify_manifest(manifest: &ArtifactManifest, root: &Path) -> Result<(), MissionSupervisorError> {
    if manifest.schema_version != 1
        || manifest.target_os != current_target_os()
        || manifest.arch != current_arch()
        || !is_hex(&manifest.source_revision, &[40, 64])
        || !is_hex(&manifest.uv_lock_sha256, &[64])
    {
        return Err(manifest_error(
            "manifest identity does not match this runtime",
        ));
    }
    let canonical_root = fs::canonicalize(root).map_err(io_error)?;
    let mut seen = HashSet::with_capacity(manifest.files.len());
    for entry in &manifest.files {
        verify_manifest_entry(entry, &canonical_root, &mut seen)?;
    }
    Ok(())
}

fn verify_manifest_entry(
    entry: &ManifestEntry,
    root: &Path,
    seen: &mut HashSet<String>,
) -> Result<(), MissionSupervisorError> {
    let relative = Path::new(&entry.path);
    if !safe_relative(relative) || !seen.insert(entry.path.clone()) {
        return Err(manifest_error("unsafe or duplicate manifest path"));
    }
    let path = root.join(relative);
    match entry.kind {
        ManifestKind::File => verify_regular_file(entry, &path),
        ManifestKind::Symlink => verify_symlink(entry, root, &path),
    }
}

fn verify_regular_file(entry: &ManifestEntry, path: &Path) -> Result<(), MissionSupervisorError> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    let digest = sha256_file(path).map_err(io_error)?;
    if !metadata.file_type().is_file()
        || entry.size != Some(metadata.len())
        || entry.sha256.as_deref() != Some(&digest)
        || entry.link_target.is_some()
    {
        return Err(manifest_error("manifest file metadata changed"));
    }
    Ok(())
}

fn verify_symlink(
    entry: &ManifestEntry,
    root: &Path,
    path: &Path,
) -> Result<(), MissionSupervisorError> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    let target = fs::read_link(path).map_err(io_error)?;
    let expected = entry.link_target.as_deref().map(Path::new);
    if !metadata.file_type().is_symlink()
        || entry.size.is_some()
        || entry.sha256.is_some()
        || expected != Some(target.as_path())
    {
        return Err(manifest_error("manifest symlink metadata changed"));
    }
    let resolved =
        fs::canonicalize(path.parent().unwrap_or(root).join(target)).map_err(io_error)?;
    if !resolved.starts_with(root) {
        return Err(manifest_error("manifest symlink escapes helper root"));
    }
    Ok(())
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn current_target_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

fn is_hex(value: &str, lengths: &[usize]) -> bool {
    lengths.contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn manifest_error(message: impl Into<String>) -> MissionSupervisorError {
    MissionSupervisorError::Manifest(message.into())
}

fn io_error(error: io::Error) -> MissionSupervisorError {
    MissionSupervisorError::Io(error.to_string())
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(hash.finalize_hex())
}

struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    block_len: usize,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            block_len: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.total_len = self.total_len.saturating_add(input.len() as u64);
        if self.block_len > 0 {
            let count = input.len().min(64 - self.block_len);
            self.block[self.block_len..self.block_len + count].copy_from_slice(&input[..count]);
            self.block_len += count;
            input = &input[count..];
            if self.block_len == 64 {
                compress(&mut self.state, &self.block);
                self.block_len = 0;
            }
        }
        while input.len() >= 64 {
            let block: &[u8; 64] = input[..64].try_into().expect("exact SHA-256 block");
            compress(&mut self.state, block);
            input = &input[64..];
        }
        self.block[..input.len()].copy_from_slice(input);
        self.block_len = input.len();
    }

    fn finalize_hex(mut self) -> String {
        let bit_len = self.total_len.saturating_mul(8);
        self.block[self.block_len] = 0x80;
        self.block_len += 1;
        if self.block_len > 56 {
            self.block[self.block_len..].fill(0);
            compress(&mut self.state, &self.block);
            self.block = [0; 64];
        } else {
            self.block[self.block_len..56].fill(0);
        }
        self.block[56..].copy_from_slice(&bit_len.to_be_bytes());
        compress(&mut self.state, &self.block);
        self.state
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect()
    }
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut words = [0u32; 64];
    for (index, chunk) in block.chunks_exact(4).enumerate() {
        words[index] = u32::from_be_bytes(chunk.try_into().expect("four SHA-256 bytes"));
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let mut work = *state;
    for index in 0..64 {
        sha256_round(&mut work, words[index], SHA256_K[index]);
    }
    for index in 0..8 {
        state[index] = state[index].wrapping_add(work[index]);
    }
}

fn sha256_round(work: &mut [u32; 8], word: u32, constant: u32) {
    let choice = (work[4] & work[5]) ^ ((!work[4]) & work[6]);
    let majority = (work[0] & work[1]) ^ (work[0] & work[2]) ^ (work[1] & work[2]);
    let sum0 = work[0].rotate_right(2) ^ work[0].rotate_right(13) ^ work[0].rotate_right(22);
    let sum1 = work[4].rotate_right(6) ^ work[4].rotate_right(11) ^ work[4].rotate_right(25);
    let first = work[7]
        .wrapping_add(sum1)
        .wrapping_add(choice)
        .wrapping_add(constant)
        .wrapping_add(word);
    let second = sum0.wrapping_add(majority);
    work.copy_within(0..7, 1);
    work[4] = work[4].wrapping_add(first);
    work[0] = first.wrapping_add(second);
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_public_empty_digest() {
        let mut hash = Sha256::new();
        hash.update(b"");
        assert_eq!(
            hash.finalize_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
