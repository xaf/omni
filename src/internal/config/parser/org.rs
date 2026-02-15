/// OrgConfig using compote's derive macro.
///
/// This configuration represents an organization with its settings.
/// It can be parsed from either a full table or a simple string format "handle=worktree".
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations (e.g., cache files).
#[derive(Debug, Clone, compote::Config)]
pub struct OrgConfig {
    pub handle: String,

    #[compote(default = "false")]
    pub trusted: bool,

    pub worktree: Option<String>,

    pub repo_path_format: Option<String>,
}

impl Default for OrgConfig {
    fn default() -> Self {
        Self {
            handle: "".to_string(),
            trusted: false,
            worktree: None,
            repo_path_format: None,
        }
    }
}

impl OrgConfig {
    pub fn from_str(value_str: &str) -> Self {
        let mut split = value_str.split('=');
        let handle = split.next().unwrap().to_string();
        let worktree = split.next().map(|value| value.to_string());
        Self {
            handle,
            trusted: true,
            worktree,
            repo_path_format: None,
        }
    }
}

