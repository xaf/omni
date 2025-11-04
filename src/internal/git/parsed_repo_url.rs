use crate::internal::errors::GitUrlError;
use git_url_parse::types::provider::AzureDevOpsProvider;
use git_url_parse::types::provider::GenericProvider;
use git_url_parse::types::provider::GitLabProvider;
use git_url_parse::GitUrl;
use regex::Regex;
use url::Url;

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
    pub branch: Option<String>, // TODO: rename to ref in a future refactor
    pub path: Option<String>,
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

        // Try to parse as a recognized web URL format first
        if let Some(parsed) = Self::try_parse_web_url(&raw, url) {
            return parsed;
        }

        // Fall back to generic parsing logic
        let branch = None;
        let path = None;
        let cleaned_path = url.path().to_string();

        let (mut owner, mut name) = if let Ok(p) = url.provider_info::<AzureDevOpsProvider>() {
            (
                Some(format!("{}/{}", p.org(), p.project())),
                p.repo().to_string(),
            )
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
        let mut parts: Vec<&str> = cleaned_path
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
            branch,
            path,
        }
    }

    /// Try to parse a recognized web URL format from various git hosting providers.
    /// Returns Some(ParsedRepoUrl) if the URL matches a known web pattern, None otherwise.
    ///
    /// This handles specific URL patterns like:
    /// - GitHub: /owner/repo/tree/branch/path
    /// - GitLab: /owner/repo/-/blob/branch/path
    /// - Bitbucket Server: /projects/owner/repos/repo/browse/path?at=branch
    /// - Azure DevOps: /org/project/_git/repo?version=GBbranch&path=/path
    /// - And many more...
    ///
    /// Uses regex patterns ordered from most to least specific.
    fn try_parse_web_url(raw_url: &str, git_url: &GitUrl) -> Option<Self> {
        let original_path = git_url.path();
        let scheme = git_url.scheme();

        // Only parse HTTP(S) URLs
        if !matches!(scheme, Some("http") | Some("https")) {
            return None;
        }

        // Helper to strip refs/heads/ prefix from ref name
        let strip_refs_heads =
            |r: &str| -> String { r.strip_prefix("refs/heads/").unwrap_or(r).to_string() };

        // Prepare a base mutable result that we'll populate
        let mut result = Self {
            raw: raw_url.to_string(),
            scheme: git_url.scheme().map(|s| s.to_string()),
            host: git_url.host().map(|s| s.to_string()),
            port: git_url.port(),
            user: git_url.user().map(|s| s.to_string()),
            password: git_url.password().map(|s| s.to_string()),
            owner: None,
            name: String::new(),
            git_suffix: git_url.path().ends_with(".git"),
            print_scheme: git_url.print_scheme(),
            branch: None,
            path: None,
        };

        // Special preprocessing for raw.githubusercontent.com
        if git_url.host() == Some("raw.githubusercontent.com") {
            result.host = Some("github.com".to_string()); // Change host to github.com

            // Pattern: /<owner>/<repo>/(refs/heads/)?<ref>/<path>
            if let Ok(re) = Regex::new(r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/(refs/heads/)?(?P<ref>[^/]+)/(?P<path>.+)$") {
                if let Some(caps) = re.captures(original_path) {
                    result.owner = caps.name("owner").map(|m| m.as_str().to_string());
                    result.name = caps.name("name").map(|m| m.as_str().to_string()).unwrap_or_default();
                    result.branch = caps.name("ref").map(|m| strip_refs_heads(m.as_str()));
                    result.path = caps.name("path").map(|m| m.as_str().to_string());
                    return Some(result);
                }
            }

            // If raw.githubusercontent.com pattern doesn't match, return None
            return None;
        }

        // Helper to construct ParsedRepoUrl from extracted components
        let build_result = |branch: Option<String>,
                            path: Option<String>,
                            owner: Option<String>,
                            mut name: String|
         -> Self {
            // Strip .git suffix from name
            if let Some(stripped) = name.strip_suffix(".git") {
                name = stripped.to_string();
            }

            Self {
                raw: raw_url.to_string(),
                scheme: git_url.scheme().map(|s| s.to_string()),
                host: git_url.host().map(|s| s.to_string()),
                port: git_url.port(),
                user: git_url.user().map(|s| s.to_string()),
                password: git_url.password().map(|s| s.to_string()),
                owner,
                name,
                git_suffix: git_url.path().ends_with(".git"),
                print_scheme: git_url.print_scheme(),
                branch,
                path,
            }
        };

        // Type alias for query parameter checker function
        type QueryParamChecker = fn(&Url) -> Option<(Option<String>, Option<String>)>;

        // Regex patterns ordered from most to least specific
        // Format: (regex_pattern, description, check_query_params_fn)
        let patterns: Vec<(&str, &str, Option<QueryParamChecker>)> = vec![
            // Google Cloud Source: /<owner>/<repo>/+/<ref>:<path> (colon separator)
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/\+/(?P<ref>[^:]+):(?P<path>.+)$",
                "google_cloud_colon",
                None,
            ),
            // Google Cloud Source: /<owner>/<repo>/+/<ref>/<path> or /+/<ref>
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/\+/(?P<ref>refs/heads/[^/]+|[^/]+)(/(?P<path>.+))?$",
                "google_cloud",
                None,
            ),
            // Google Cloud Source: /<repo>/+/<ref>:<path> (single-repo, no owner)
            (
                r"^/(?P<name>[^/]+)/\+/(?P<ref>[^:]+):(?P<path>.+)$",
                "google_cloud_single_colon",
                None,
            ),
            // Google Cloud Source: /<repo>/+/<ref> (single-repo, no owner)
            (
                r"^/(?P<name>[^/]+)/\+/(?P<ref>refs/heads/[^/]+|[^/]+)(/(?P<path>.+))?$",
                "google_cloud_single",
                None,
            ),
            // AWS CodeCommit: /codesuite/codecommit/repositories/<repo>/browse/refs/heads/<ref>/--/<path>
            (
                r"^/codesuite/codecommit/repositories/(?P<name>[^/]+)/browse/refs/heads/(?P<ref>[^/]+)/--/(?P<path>.+)$",
                "aws_browse",
                None,
            ),
            (
                r"^/codesuite/codecommit/repositories/(?P<name>[^/]+)/browse/refs/heads/(?P<ref>[^/]+)/?$",
                "aws_browse_no_path",
                None,
            ),
            // AWS CodeCommit: commit URL
            (
                r"^/codesuite/codecommit/repositories/(?P<name>[^/]+)/commit/(?P<ref>[^/]+)$",
                "aws_commit",
                None,
            ),
            // Azure DevOps: /org/project/_git/repo with query params
            (
                r"^/(?P<owner>[^/]+/[^/]+)/_git/(?P<name>[^/?]+)",
                "azure_devops",
                Some(|url: &Url| {
                    let mut git_ref = None;
                    let mut path = None;
                    for (key, value) in url.query_pairs() {
                        match key.as_ref() {
                            "version" if value.starts_with("GB") => {
                                git_ref = Some(value[2..].to_string());
                            }
                            "path" => {
                                path = Some(value.strip_prefix('/').unwrap_or(&value).to_string());
                            }
                            _ => {}
                        }
                    }
                    if git_ref.is_some() || path.is_some() {
                        Some((git_ref, path))
                    } else {
                        None
                    }
                }),
            ),
            // Bitbucket Server: /projects/<owner>/repos/<repo>/browse/<path> with ?at=<ref>
            (
                r"^/projects/(?P<owner>[^/]+)/repos/(?P<name>[^/]+)/(browse|raw)/(?P<path>[^?]+)",
                "bitbucket_server",
                Some(|url: &Url| {
                    url.query_pairs().find(|(k, _)| k == "at").map(|(_, v)| {
                        let git_ref = v.strip_prefix("refs/heads/").unwrap_or(&v).to_string();
                        (Some(git_ref), None) // path is in the URL path, not query
                    })
                }),
            ),
            // SourceForge: /p/<owner>/<repo>/ci/<ref>/tree/<path>
            (
                r"^/p/(?P<owner>[^/]+)/(?P<name>[^/]+)/ci/(?P<ref>[^/]+)/tree/(?P<path>.+)$",
                "sourceforge_tree",
                None,
            ),
            (
                r"^/p/(?P<owner>[^/]+)/(?P<name>[^/]+)/ci/(?P<ref>[^/]+)/?$",
                "sourceforge",
                None,
            ),
            // Gitea specific patterns with subgroups: /group1/group2/repo/...
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/src/branch/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_src_branch_subgroup",
                None,
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/src/commit/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_src_commit_subgroup",
                None,
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/raw/branch/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_raw_branch_subgroup",
                None,
            ),
            // Gitea standard: /owner/repo/...
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/src/branch/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_src_branch",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/src/commit/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_src_commit",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/raw/branch/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_raw_branch",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/raw/commit/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_raw_commit",
                None,
            ),
            // GitLab patterns with /-/ separator and subgroups: /group1/group2.../repo/-/...
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/tree/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_tree_subgroup",
                None,
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/blob/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_blob_subgroup",
                None,
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/raw/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_raw_subgroup",
                None,
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/commit/(?P<ref>[^/]+)$",
                "gitlab_commit_subgroup",
                None,
            ),
            // GitLab standard: /owner/repo/-/...
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/tree/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_tree",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/blob/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_blob",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/raw/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_raw",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/commit/(?P<ref>[^/]+)$",
                "gitlab_commit",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/commits/(?P<ref>[^/]+)$",
                "gitlab_commits",
                None,
            ),
            // GitHub standard patterns
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/tree/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "github_tree",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/blob/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "github_blob",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/commit/(?P<ref>[^/]+)$",
                "github_commit",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/commits/(?P<ref>[^/]+)$",
                "github_commits",
                None,
            ),
            // Bitbucket Cloud (least specific)
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/src/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "bitbucket_src",
                None,
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/raw/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "bitbucket_raw",
                None,
            ),
        ];

        // Try each pattern in order
        for (pattern_str, _desc, query_check_fn) in patterns {
            let re = match Regex::new(pattern_str) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if let Some(caps) = re.captures(original_path) {
                // Extract owner from various possible capture groups
                let owner = if let Some(owner_cap) = caps.name("owner") {
                    Some(owner_cap.as_str().to_string())
                } else if let (Some(org), Some(project)) = (caps.name("org"), caps.name("project"))
                {
                    // Azure DevOps: org/project
                    Some(format!("{}/{}", org.as_str(), project.as_str()))
                } else if let (Some(group1), Some(group2)) =
                    (caps.name("group1"), caps.name("group2"))
                {
                    // GitLab/Gitea subgroups: group1/group2
                    Some(format!("{}/{}", group1.as_str(), group2.as_str()))
                } else {
                    None
                };

                // Extract name (required)
                let name = caps
                    .name("name")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                // If this pattern requires query parameter checking, do it now
                if let Some(check_fn) = query_check_fn {
                    let parsed_url = match Url::parse(raw_url) {
                        Ok(url) => url,
                        Err(_) => continue,
                    };

                    if let Some((query_ref, query_path)) = check_fn(&parsed_url) {
                        // For Bitbucket Server, path comes from URL path, ref from query
                        let path_from_url = caps.name("path").map(|m| m.as_str().to_string());
                        let final_path = query_path.or(path_from_url);

                        return Some(build_result(query_ref, final_path, owner, name));
                    } else {
                        // Query check failed, try next pattern
                        continue;
                    }
                }

                // Extract ref and path from regex captures
                let git_ref = caps.name("ref").map(|m| strip_refs_heads(m.as_str()));
                let path = caps.name("path").map(|m| m.as_str().to_string());

                return Some(build_result(git_ref, path, owner, name));
            }
        }

        // No pattern matched, return None to use fallback parsing
        None
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
