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

        let (mut owner, mut name) = if let Ok(p) = url.provider_info::<AzureDevOpsProvider>() {
            (Some(format!("{}/{}", p.org(), p.project())), p.repo().to_string())
        } else if let Ok(p) = url.provider_info::<GitLabProvider>() {
            // Owner includes subgroups, matching older GitUrl semantics
            let fullname = p.fullname(); // e.g., owner/group1/group2/repo
            let repo = p.repo().to_string();
            let owner = fullname
                .strip_suffix(&format!("/{}", repo))
                .unwrap_or(&fullname)
                .to_string();
            (Some(owner), repo)
        } else if let Ok(p) = url.provider_info::<GenericProvider>() {
            (Some(p.owner().to_string()), p.repo().to_string())
        } else {
            (None, String::new())
        };

        // Normalize owner/name deterministically per-host
        let host_str = url.host().unwrap_or("");
        let mut parts: Vec<&str> = url
            .path()
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if matches!(host_str, "dev.azure.com" | "ssh.dev.azure.com") {
            strip_azure_version_prefix(&mut parts);
        }
        if let Some(last) = parts.last_mut() {
            if let Some(stripped) = last.strip_suffix(".git") {
                *last = stripped;
            }
        }
        if host_str == "dev.azure.com" || host_str == "ssh.dev.azure.com" {
            // owner = Org/Project when available; name only if Repo present
            if parts.len() >= 2 {
                owner = Some(format!("{}/{}", parts[0], parts[1]));
            } else if parts.len() == 1 {
                owner = Some(parts[0].to_string());
            }
            name.clear();
            if let Some(idx) = parts.iter().position(|s| *s == "_git") {
                if idx + 1 < parts.len() {
                    name = parts[idx + 1].to_string();
                }
            } else if parts.len() >= 3 {
                name = parts[2].to_string();
            }
        } else if host_str == "gitlab.com" {
            // owner = full namespace; name = leaf
            if parts.len() >= 2 {
                name = parts.last().unwrap().to_string();
                owner = Some(parts[..parts.len() - 1].join("/"));
            } else if parts.len() == 1 {
                owner = Some(parts[0].to_string());
                name.clear();
            }
        } else {
            // Generic/GitHub
            if parts.len() >= 2 {
                name = parts.last().unwrap().to_string();
                owner = Some(parts[..parts.len() - 1].join("/"));
            } else if parts.len() == 1 {
                owner = Some(parts[0].to_string());
                name.clear();
            }
        }

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

pub(crate) fn strip_azure_version_prefix(parts: &mut Vec<&str>) {
    if !parts.is_empty() {
        let first = parts[0];
        if first.len() > 1
            && first.starts_with('v')
            && first[1..].chars().all(|c| c.is_ascii_digit())
        {
            parts.remove(0);
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

#[cfg(test)]
#[path = "parsed_repo_url_test.rs"]
mod tests;
