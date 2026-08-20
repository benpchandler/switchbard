//! When this machine last booted, as unix seconds.
//!
//! Exists for exactly one reason: **pid recycling**. A process id (and
//! therefore a process *group* id) is only meaningful within one boot — macOS
//! wraps at 99999 and reuses ids aggressively, so a number recorded before a
//! reboot names some unrelated process afterwards. Anything that persists a
//! pid to disk and later signals it has to record the boot it belongs to, or
//! it is one restart away from killing a stranger. See
//! [`crate::dispatch::DispatchSidecar`], the one caller.
//!
//! The value is read once and cached: a running process cannot outlive the
//! boot it is running under, so re-reading it would be re-answering a
//! question whose answer cannot change.
//!
//! Both parsers are compiled and tested on both platforms even though only
//! one is *called* per platform. The `cfg` is on the acquisition (which file
//! or command supplies the text), never on the parsing — so a Linux CI run
//! still proves the macOS parser against real captured `sysctl` output, and
//! vice versa.

use std::sync::OnceLock;

/// Unix seconds at which this machine booted, or `None` if the platform
/// wouldn't say.
///
/// `None` is a real answer, not an error to paper over: a caller that cannot
/// establish the boot epoch cannot verify a persisted pid either, and must
/// fail closed rather than guess.
pub fn boot_epoch_unix() -> Option<u64> {
    static CACHED: OnceLock<Option<u64>> = OnceLock::new();
    *CACHED.get_or_init(read_boot_epoch_unix)
}

#[cfg(target_os = "macos")]
fn read_boot_epoch_unix() -> Option<u64> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_darwin_boottime(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(target_os = "macos"))]
fn read_boot_epoch_unix() -> Option<u64> {
    parse_proc_stat_btime(&std::fs::read_to_string("/proc/stat").ok()?)
}

/// Pull the seconds out of Darwin's `sysctl -n kern.boottime`, which prints
/// e.g. `{ sec = 1786216693, usec = 66375 } Sat Aug  8 15:18:13 2026`.
///
/// Only `sec` is taken. `usec` is sub-second jitter on a value used solely as
/// an equality token between two runs on the same boot, and the trailing
/// human date is a formatting of the same number in a locale-dependent form —
/// parsing either would add failure modes without adding information.
pub fn parse_darwin_boottime(text: &str) -> Option<u64> {
    let after = text.split("sec").nth(1)?;
    let digits: String = after
        .trim_start_matches(|c: char| c == '=' || c.is_whitespace())
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok().filter(|secs: &u64| *secs > 0)
}

/// Pull `btime <unix seconds>` out of Linux's `/proc/stat`. The field is
/// documented as the boot timestamp in seconds since the epoch and sits
/// among many other lines, so this scans for the key rather than assuming a
/// position.
pub fn parse_proc_stat_btime(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()
        .filter(|secs: &u64| *secs > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real output captured from `sysctl -n kern.boottime` on macOS 15.
    const DARWIN_SAMPLE: &str = "{ sec = 1786216693, usec = 66375 } Sat Aug  8 15:18:13 2026\n";

    #[test]
    fn darwin_boottime_yields_the_seconds_field() {
        assert_eq!(parse_darwin_boottime(DARWIN_SAMPLE), Some(1_786_216_693));
    }

    /// Some releases print the struct without spaces around `=`; the parser
    /// must not depend on the exact whitespace of a human-facing format.
    #[test]
    fn darwin_boottime_tolerates_whitespace_variation() {
        assert_eq!(
            parse_darwin_boottime("{ sec=1786216693, usec=1 }"),
            Some(1_786_216_693)
        );
    }

    /// `usec` also contains the substring `sec`; taking the *first* split is
    /// what keeps this reading the boot seconds rather than the microseconds.
    #[test]
    fn darwin_boottime_does_not_read_the_usec_field() {
        assert_ne!(parse_darwin_boottime(DARWIN_SAMPLE), Some(66_375));
    }

    #[test]
    fn darwin_boottime_rejects_junk_and_zero() {
        assert_eq!(parse_darwin_boottime(""), None);
        assert_eq!(parse_darwin_boottime("no fields here"), None);
        assert_eq!(parse_darwin_boottime("{ sec = , usec = 1 }"), None);
        assert_eq!(parse_darwin_boottime("{ sec = 0, usec = 1 }"), None);
    }

    /// Real `/proc/stat` shape: btime is one line among many and is not first.
    #[test]
    fn proc_stat_btime_is_found_among_the_other_lines() {
        let sample =
            "cpu  1 2 3 4\ncpu0 1 2 3 4\nintr 999\nctxt 12345\nbtime 1786216693\nprocesses 77\n";
        assert_eq!(parse_proc_stat_btime(sample), Some(1_786_216_693));
    }

    #[test]
    fn proc_stat_btime_rejects_a_file_without_it() {
        assert_eq!(parse_proc_stat_btime("cpu  1 2 3 4\nctxt 5\n"), None);
        assert_eq!(parse_proc_stat_btime("btime notanumber\n"), None);
        assert_eq!(parse_proc_stat_btime("btime 0\n"), None);
    }

    /// Pins that the prefix match requires the trailing space, so a future
    /// `btimefoo` key can't be misread as `btime`.
    #[test]
    fn proc_stat_btime_requires_the_exact_key() {
        assert_eq!(parse_proc_stat_btime("btimefoo 1786216693\n"), None);
    }

    /// The live value must be plausible: after 2020 and not in the future.
    /// This is the one assertion that catches a platform whose acquisition
    /// path silently changed shape.
    #[test]
    fn the_live_boot_epoch_is_plausible() {
        let Some(boot) = boot_epoch_unix() else {
            // A platform that won't say is handled by callers failing closed.
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(boot > 1_577_836_800, "boot epoch before 2020: {boot}");
        assert!(boot <= now, "boot epoch in the future: {boot} > {now}");
    }

    /// Cached: a running process cannot outlive its own boot.
    #[test]
    fn the_boot_epoch_is_stable_within_one_process() {
        assert_eq!(boot_epoch_unix(), boot_epoch_unix());
    }
}
