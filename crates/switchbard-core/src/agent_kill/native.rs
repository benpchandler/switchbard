//! Native fixed-buffer process identity reads; no subprocess waits.
use anyhow::{Context, Result};
use std::path::PathBuf;

pub(super) struct Facts {
    pub start: (u64, u64),
    pub started_unix: Option<u64>,
    pub cwd: PathBuf,
    pub executable: PathBuf,
}

#[cfg(target_os = "macos")]
fn info<T>(pid: u32, flavor: i32) -> Result<T> {
    // SAFETY: callers use libc integer-only C structs valid when zeroed.
    // The writable buffer is exactly sizeof(T), alive for the entire call;
    // proc_pidinfo accepts that size and no reference aliases the write.
    let mut value: T = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<T>() as i32;
    let count =
        unsafe { libc::proc_pidinfo(pid as i32, flavor, 0, (&mut value as *mut T).cast(), size) };
    if count != size {
        return Err(std::io::Error::last_os_error()).context("Cannot read native process identity");
    }
    Ok(value)
}

#[cfg(target_os = "macos")]
pub(super) fn parent(pid: u32) -> Result<u32> {
    Ok(info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO)?.pbi_ppid)
}

#[cfg(target_os = "macos")]
pub(super) fn probe(pid: u32) -> Result<Facts> {
    let before = info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO)?;
    let vnode = info::<libc::proc_vnodepathinfo>(pid, libc::PROC_PIDVNODEPATHINFO)?;
    let mut executable = [0u8; 4096];
    // SAFETY: positive range-validated pid; fixed writable byte array lives
    // through the call, has the declared size, and contains no Rust pointers.
    let count = unsafe {
        libc::proc_pidpath(
            pid as i32,
            executable.as_mut_ptr().cast(),
            executable.len() as u32,
        )
    };
    if count <= 0 {
        return Err(std::io::Error::last_os_error()).context("Cannot read process executable");
    }
    let cwd: Vec<u8> = vnode
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .map(|c| *c as u8)
        .take_while(|c| *c != 0)
        .collect();
    let after = info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO)?;
    anyhow::ensure!(
        before.pbi_pid == pid
            && after.pbi_pid == pid
            && before.pbi_start_tvsec == after.pbi_start_tvsec
            && before.pbi_start_tvusec == after.pbi_start_tvusec,
        "Process changed during identity read"
    );
    use std::os::unix::ffi::OsStringExt;
    anyhow::ensure!(!cwd.is_empty(), "Process cwd unavailable");
    let executable = executable.into_iter().take_while(|c| *c != 0).collect();
    Ok(Facts {
        start: (before.pbi_start_tvsec, before.pbi_start_tvusec),
        started_unix: Some(before.pbi_start_tvsec),
        cwd: std::ffi::OsString::from_vec(cwd).into(),
        executable: std::ffi::OsString::from_vec(executable).into(),
    })
}

#[cfg(target_os = "linux")]
fn stat(pid: u32) -> Result<(u32, u64)> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(format!("/proc/{pid}/stat"))?
        .take(8193)
        .read_to_string(&mut text)?;
    anyhow::ensure!(text.len() <= 8192, "Process stat exceeds bound");
    let (_, fields) = text.rsplit_once(')').context("Invalid process stat")?;
    let fields: Vec<_> = fields.split_whitespace().take(23).collect();
    Ok((
        fields.get(1).context("Missing process parent")?.parse()?,
        fields.get(19).context("Missing process start")?.parse()?,
    ))
}

#[cfg(target_os = "linux")]
pub(super) fn parent(pid: u32) -> Result<u32> {
    Ok(stat(pid)?.0)
}

#[cfg(target_os = "linux")]
pub(super) fn probe(pid: u32) -> Result<Facts> {
    let before = stat(pid)?;
    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd"))?;
    let executable = std::fs::read_link(format!("/proc/{pid}/exe"))?;
    anyhow::ensure!(before == stat(pid)?, "Process changed during identity read");
    Ok(Facts {
        start: (before.1, 0),
        started_unix: linux_birth(before.1),
        cwd,
        executable,
    })
}

#[cfg(target_os = "linux")]
fn linux_birth(ticks: u64) -> Option<u64> {
    use std::io::Read;
    static BOOT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let boot = BOOT.get_or_init(|| {
        let mut text = String::new();
        std::fs::File::open("/proc/stat")
            .ok()?
            .take(1_048_577)
            .read_to_string(&mut text)
            .ok()?;
        (text.len() <= 1_048_576)
            .then(|| crate::boot_time::parse_proc_stat_btime(&text))
            .flatten()
    });
    // SAFETY: sysconf reads one fixed platform constant, with no pointers.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if hz <= 0 {
        return None;
    }
    boot.and_then(|boot| boot.checked_add(ticks / hz as u64))
}
