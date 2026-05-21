//! Open a URL in the user's default browser, or a specific browser app on macOS.

use std::io;
use std::process::Command;

/// Common browsers on macOS — used to populate the GUI dropdown. The first
/// entry is `None` meaning "system default".
pub const BROWSER_APP_NAMES: &[&str] = &[
    "Safari",
    "Google Chrome",
    "Firefox",
    "Arc",
    "Brave Browser",
    "Microsoft Edge",
];

pub fn open_url(url: &str, browser_app: Option<&str>) -> io::Result<()> {
    let mut cmd = Command::new("open");
    if let Some(app) = browser_app {
        cmd.arg("-a").arg(app);
    }
    cmd.arg(url);
    cmd.spawn()?;
    Ok(())
}

/// Best-guess URL for a port. Most local dev servers respond on http; HTTPS is
/// rare for development. We return `http://localhost:<port>` unconditionally.
pub fn url_for_port(port: u16) -> String {
    format!("http://localhost:{port}")
}
