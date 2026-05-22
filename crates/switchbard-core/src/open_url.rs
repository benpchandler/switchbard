//! Open a URL in the user's default browser, or a specific browser executable.

use std::io;
use std::process::Command;

/// Common browsers on macOS — used to populate the GUI dropdown.
#[cfg(target_os = "macos")]
pub const BROWSER_APP_NAMES: &[&str] = &[
    "Safari",
    "Google Chrome",
    "Firefox",
    "Arc",
    "Brave Browser",
    "Microsoft Edge",
];

/// Common browser executable names on Linux. The first GUI entry is still
/// "Default"; choosing one of these bypasses xdg-open and runs it directly.
#[cfg(target_os = "linux")]
pub const BROWSER_APP_NAMES: &[&str] = &[
    "firefox",
    "google-chrome",
    "chromium",
    "brave-browser",
    "microsoft-edge",
];

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub const BROWSER_APP_NAMES: &[&str] = &[];

pub fn open_url(url: &str, browser_app: Option<&str>) -> io::Result<()> {
    let spec = launch_spec(url, browser_app);
    Command::new(spec.program).args(spec.args).spawn()?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct LaunchSpec<'a> {
    program: &'a str,
    args: Vec<&'a str>,
}

#[cfg(target_os = "macos")]
fn launch_spec<'a>(url: &'a str, browser_app: Option<&'a str>) -> LaunchSpec<'a> {
    let mut args = Vec::new();
    if let Some(app) = browser_app {
        args.push("-a");
        args.push(app);
    }
    args.push(url);
    LaunchSpec {
        program: "open",
        args,
    }
}

#[cfg(target_os = "linux")]
fn launch_spec<'a>(url: &'a str, browser_app: Option<&'a str>) -> LaunchSpec<'a> {
    match browser_app {
        Some(app) => LaunchSpec {
            program: app,
            args: vec![url],
        },
        None => LaunchSpec {
            program: "xdg-open",
            args: vec![url],
        },
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn launch_spec<'a>(url: &'a str, browser_app: Option<&'a str>) -> LaunchSpec<'a> {
    match browser_app {
        Some(app) => LaunchSpec {
            program: app,
            args: vec![url],
        },
        None => LaunchSpec {
            program: "xdg-open",
            args: vec![url],
        },
    }
}

/// Best-guess URL for a port. Most local dev servers respond on http; HTTPS is
/// rare for development. We return `http://localhost:<port>` unconditionally.
pub fn url_for_port(port: u16) -> String {
    format!("http://localhost:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_url_is_localhost_http() {
        assert_eq!(url_for_port(5173), "http://localhost:5173");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_open_with_optional_app() {
        assert_eq!(
            launch_spec("http://localhost:3000", None),
            LaunchSpec {
                program: "open",
                args: vec!["http://localhost:3000"],
            }
        );
        assert_eq!(
            launch_spec("http://localhost:3000", Some("Safari")),
            LaunchSpec {
                program: "open",
                args: vec!["-a", "Safari", "http://localhost:3000"],
            }
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_uses_xdg_open_or_browser_executable() {
        assert_eq!(
            launch_spec("http://localhost:3000", None),
            LaunchSpec {
                program: "xdg-open",
                args: vec!["http://localhost:3000"],
            }
        );
        assert_eq!(
            launch_spec("http://localhost:3000", Some("firefox")),
            LaunchSpec {
                program: "firefox",
                args: vec!["http://localhost:3000"],
            }
        );
    }
}
