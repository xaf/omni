use git_url_parse::GitUrl;
use git_url_parse::types::provider::{AzureDevOpsProvider, GenericProvider, GitLabProvider};
use crate::internal::errors::GitUrlError;

#[derive(Debug, Clone)]
pub struct ParsedRepoUrl {
    pub raw: String,
    pub scheme: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub owner: Option<String>,
    pub name: String,
    pub git_suffix: bool,
    pub print_scheme: bool,
}

impl ParsedRepoUrl {
    pub fn parse(input: &str) -> Result<Self, GitUrlError> {
        let url = GitUrl::parse(input).map_err(GitUrlError::from)?;
        Ok(Self::from_git_url(&url, input.to_string()))
    }

    pub fn from_git_url(url: &GitUrl, raw: String) -> Self {
        let host = url.host().map(|s| s.to_string());
        let port = url.port();
        let scheme = url.scheme().map(|s| s.to_string());
        let user = url.user().map(|s| s.to_string());
        let password = url.password().map(|s| s.to_string());
        let git_suffix = url.path().ends_with(".git");
        let print_scheme = url.print_scheme();

        let (owner, name) = if let Ok(p) = url.provider_info::<GenericProvider>() {
            (Some(p.owner().to_string()), p.repo().to_string())
        } else if let Ok(p) = url.provider_info::<GitLabProvider>() {
            // Owner includes subgroups, matching older GitUrl semantics
            let fullname = p.fullname(); // e.g., owner/group1/group2/repo
            let repo = p.repo().to_string();
            let owner = fullname
                .strip_suffix(&format!("/{}", repo))
                .unwrap_or(&fullname)
                .to_string();
            (Some(owner), repo)
        } else if let Ok(p) = url.provider_info::<AzureDevOpsProvider>() {
            // Owner is org/project for Azure DevOps
            (Some(format!("{}/{}", p.org(), p.project())), p.repo().to_string())
        } else {
            (None, String::new())
        };

        Self {
            raw,
            scheme,
            host,
            port,
            user,
            password,
            owner,
            name,
            git_suffix,
            print_scheme,
        }
    }
}

// Keep the external error type usage confined to this file by implementing
// a conversion into our internal error without exposing the external type.
impl From<git_url_parse::GitUrlParseError> for GitUrlError {
    fn from(err: git_url_parse::GitUrlParseError) -> Self {
        GitUrlError::GitUrlParse(err.to_string())
    }
}
