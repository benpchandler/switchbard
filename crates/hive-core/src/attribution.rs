use crate::types::{AttributedListener, LocalListener, Repo};

pub fn attribute(listeners: &[LocalListener], repos: &[Repo]) -> Vec<AttributedListener> {
    listeners
        .iter()
        .map(|l| AttributedListener {
            repo_name: l.cwd.as_ref().and_then(|cwd| {
                repos
                    .iter()
                    .find(|r| cwd.starts_with(&r.path))
                    .map(|r| r.name.clone())
            }),
            listener: l.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_listener(pid: u32, port: u16, cwd: Option<&str>) -> LocalListener {
        LocalListener {
            pid,
            pgid: pid as i32,
            port,
            command_name: "x".into(),
            cwd: cwd.map(PathBuf::from),
        }
    }

    #[test]
    fn matches_by_cwd_prefix() {
        let repos = vec![
            Repo {
                name: "delta".into(),
                path: PathBuf::from("/Users/me/code/delta"),
            },
            Repo {
                name: "alpha".into(),
                path: PathBuf::from("/Users/me/code/alpha"),
            },
        ];
        let listeners = vec![
            make_listener(1, 8000, Some("/Users/me/code/delta/scripts")),
            make_listener(2, 8420, Some("/Users/me/code/alpha/lyon")),
            make_listener(3, 7000, Some("/usr/bin")),
            make_listener(4, 9000, None),
        ];
        let out = attribute(&listeners, &repos);
        assert_eq!(out[0].repo_name.as_deref(), Some("delta"));
        assert_eq!(out[1].repo_name.as_deref(), Some("alpha"));
        assert_eq!(out[2].repo_name, None);
        assert_eq!(out[3].repo_name, None);
    }
}
