use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LocalListener {
    pub pid: u32,
    pub pgid: i32,
    pub port: u16,
    pub command_name: String,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AttributedListener {
    pub listener: LocalListener,
    pub repo_name: Option<String>,
}
