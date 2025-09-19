use git_url_parse::GitUrl;
use git_url_parse::types::provider::{AzureDevOpsProvider, GenericProvider, GitLabProvider};

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
    pub fn parse(input: &str) -> Result<Self, git_url_parse::GitUrlParseError> {
        let url = GitUrl::parse(input)?;
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
            (Some(p.owner().to_string()), p.repo().to_string())
        } else if let Ok(p) = url.provider_info::<AzureDevOpsProvider>() {
            (Some(p.org().to_string()), p.repo().to_string())
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

