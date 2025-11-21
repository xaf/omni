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
    pub git_ref: Option<String>,
    pub path: Option<String>,
    pub line_from: Option<u32>,
    pub line_to: Option<u32>,
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
        let git_ref = None;
        let path = None;
        let line_from = None;
        let line_to = None;
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
                // If the URL had a .git suffix, treat it as a complete repo URL
                // with no owner, otherwise treat it as owner-only (incomplete URL)
                if git_suffix {
                    name = parts[0].to_string();
                    owner = None;
                } else {
                    owner = Some(parts[0].to_string());
                    name.clear();
                }
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
            git_ref,
            path,
            line_from,
            line_to,
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
            git_ref: None,
            path: None,
            line_from: None,
            line_to: None,
        };

        // Special preprocessing for raw.githubusercontent.com
        if git_url.host() == Some("raw.githubusercontent.com") {
            result.host = Some("github.com".to_string()); // Change host to github.com

            // Pattern: /<owner>/<repo>/(refs/heads/)?<ref>/<path>
            if let Ok(re) = Regex::new(
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/(refs/heads/)?(?P<ref>[^/]+)/(?P<path>.+)$",
            ) {
                if let Some(caps) = re.captures(original_path) {
                    result.owner = caps.name("owner").map(|m| m.as_str().to_string());
                    result.name = caps
                        .name("name")
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();
                    result.git_ref = caps.name("ref").map(|m| strip_refs_heads(m.as_str()));
                    result.path = caps.name("path").map(|m| m.as_str().to_string());
                    // raw.githubusercontent.com doesn't include line numbers
                    return Some(result);
                }
            }

            // If raw.githubusercontent.com pattern doesn't match, return None
            return None;
        }

        // Helper to construct ParsedRepoUrl from extracted components
        let build_result = |git_ref: Option<String>,
                            path: Option<String>,
                            owner: Option<String>,
                            mut name: String,
                            line_from: Option<u32>,
                            line_to: Option<u32>|
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
                git_ref,
                path,
                line_from,
                line_to,
            }
        };

        // Type alias for post-processor function
        // Takes: (url, regex_captures)
        // Returns: (git_ref, path, line_from, line_to)
        type PostProcessor = fn(
            &Url,
            &regex::Captures,
        )
            -> (Option<String>, Option<String>, Option<u32>, Option<u32>);

        // Regex patterns ordered from most to least specific
        // Format: (regex_pattern, description, post_processor_fn)
        let patterns: Vec<(&str, &str, Option<PostProcessor>)> = vec![
            // Google Cloud Source: /<owner>/<repo>/+/<ref>:<path> (colon separator)
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/\+/(?P<ref>[^:]+):(?P<path>.+)$",
                "google_cloud_colon",
                None, // Google Cloud doesn't support line numbers in URLs
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
                Some(post_process_azure_devops),
            ),
            // Bitbucket Server: /projects/<owner>/repos/<repo>/commits/<ref> - check for fragment in raw URL
            (
                r"^/projects/(?P<owner>[^/]+)/repos/(?P<name>[^/]+)/commits/(?P<ref>[^?#]+)",
                "bitbucket_server_commits",
                Some(post_process_bitbucket_server_commits),
            ),
            // Bitbucket Server: /projects/<owner>/repos/<repo>/browse/<path> with ?at=<ref>
            (
                r"^/projects/(?P<owner>[^/]+)/repos/(?P<name>[^/]+)/(browse|raw)/(?P<path>[^?#]+)",
                "bitbucket_server",
                Some(post_process_bitbucket_server),
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
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/src/commit/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_src_commit_subgroup",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/raw/branch/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_raw_branch_subgroup",
                Some(post_process_github_gitlab_gitea),
            ),
            // Gitea standard: /owner/repo/...
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/src/branch/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_src_branch",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/src/commit/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_src_commit",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/raw/branch/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_raw_branch",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/raw/commit/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitea_raw_commit",
                Some(post_process_github_gitlab_gitea),
            ),
            // GitLab patterns with /-/ separator and subgroups: /group1/group2.../repo/-/...
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/tree/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_tree_subgroup",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/blob/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_blob_subgroup",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/raw/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_raw_subgroup",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<group1>[^/]+)/(?P<group2>[^/]+)/(?P<name>[^/]+)/-/commit/(?P<ref>[^/]+)$",
                "gitlab_commit_subgroup",
                Some(post_process_github_gitlab_gitea),
            ),
            // GitLab standard: /owner/repo/-/...
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/tree/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_tree",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/blob/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_blob",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/raw/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "gitlab_raw",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/commit/(?P<ref>[^/]+)$",
                "gitlab_commit",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/-/commits/(?P<ref>[^/]+)$",
                "gitlab_commits",
                Some(post_process_github_gitlab_gitea),
            ),
            // GitHub standard patterns
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/tree/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "github_tree",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/blob/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "github_blob",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/commit/(?P<ref>[^/]+)$",
                "github_commit",
                Some(post_process_github_gitlab_gitea),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/commits/(?P<ref>[^/]+)$",
                "github_commits",
                Some(post_process_github_gitlab_gitea),
            ),
            // Bitbucket Cloud (least specific)
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/src/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "bitbucket_src",
                Some(post_process_bitbucket_cloud),
            ),
            (
                r"^/(?P<owner>[^/]+)/(?P<name>[^/]+)/raw/(?P<ref>[^/]+)(/(?P<path>.+))?$",
                "bitbucket_raw",
                Some(post_process_bitbucket_cloud),
            ),
        ];

        // Try each pattern in order
        for (pattern_str, _desc, post_processor_fn) in patterns {
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

                // Extract ref and path from regex captures first
                let mut git_ref = caps.name("ref").map(|m| strip_refs_heads(m.as_str()));
                let mut path = caps.name("path").map(|m| m.as_str().to_string());
                let mut line_from = None;
                let mut line_to = None;

                // If this pattern has a post-processor, run it
                if let Some(post_proc) = post_processor_fn {
                    let parsed_url = match Url::parse(raw_url) {
                        Ok(url) => url,
                        Err(_) => {
                            // URL parsing failed, use regex captures
                            return Some(build_result(git_ref, path, owner, name, None, None));
                        }
                    };

                    let (proc_ref, proc_path, proc_line_from, proc_line_to) =
                        post_proc(&parsed_url, &caps);
                    // Override with post-processor results if present
                    git_ref = proc_ref.or(git_ref);
                    path = proc_path.or(path);
                    line_from = proc_line_from;
                    line_to = proc_line_to;
                }

                return Some(build_result(git_ref, path, owner, name, line_from, line_to));
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

// Line extraction functions for different git hosting providers
fn extract_lines_github_style(url: &Url, _path: &Option<String>) -> (Option<u32>, Option<u32>) {
    let fragment = match url.fragment() {
        Some(f) => f,
        None => return (None, None),
    };

    let re = match Regex::new(r"^L(?P<from>\d+)(-L?(?P<to>\d+))?$") {
        Ok(r) => r,
        Err(_) => return (None, None),
    };

    let caps = match re.captures(fragment) {
        Some(c) => c,
        None => return (None, None),
    };

    let from = match caps
        .name("from")
        .and_then(|m| m.as_str().parse::<u32>().ok())
    {
        Some(f) => f,
        None => return (None, None),
    };

    let to = caps
        .name("to")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .unwrap_or(from);

    (Some(from), Some(to))
}

fn extract_lines_bitbucket_cloud(url: &Url, _path: &Option<String>) -> (Option<u32>, Option<u32>) {
    let fragment = match url.fragment() {
        Some(f) => f,
        None => return (None, None),
    };

    let re = match Regex::new(r"^lines-(?P<from>\d+)(:(?P<to>\d+))?$") {
        Ok(r) => r,
        Err(_) => return (None, None),
    };

    let caps = match re.captures(fragment) {
        Some(c) => c,
        None => return (None, None),
    };

    let from = match caps
        .name("from")
        .and_then(|m| m.as_str().parse::<u32>().ok())
    {
        Some(f) => f,
        None => return (None, None),
    };

    let to = caps
        .name("to")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .unwrap_or(from);

    (Some(from), Some(to))
}

fn extract_lines_bitbucket_server(
    url: &Url,
    path_from_url: &Option<String>,
) -> (Option<u32>, Option<u32>) {
    // Only extract if we already have a path from the URL (fragment is for lines, not path)
    if path_from_url.is_none() {
        return (None, None);
    }

    let fragment = match url.fragment() {
        Some(f) => f,
        None => return (None, None),
    };

    let re = match Regex::new(r"^(?P<from>\d+)(-(?P<to>\d+))?$") {
        Ok(r) => r,
        Err(_) => return (None, None),
    };

    let caps = match re.captures(fragment) {
        Some(c) => c,
        None => return (None, None),
    };

    let from = match caps
        .name("from")
        .and_then(|m| m.as_str().parse::<u32>().ok())
    {
        Some(f) => f,
        None => return (None, None),
    };

    let to = caps
        .name("to")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .unwrap_or(from);

    (Some(from), Some(to))
}

// Post-processor functions for specific URL patterns
fn post_process_azure_devops(
    url: &Url,
    _caps: &regex::Captures,
) -> (Option<String>, Option<String>, Option<u32>, Option<u32>) {
    let mut git_ref = None;
    let mut path = None;
    let mut line_from = None;
    let mut line_to = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "version" if value.starts_with("GB") => {
                git_ref = Some(value[2..].to_string());
            }
            "path" => {
                path = Some(value.strip_prefix('/').unwrap_or(&value).to_string());
            }
            "line" => {
                line_from = value.parse::<u32>().ok();
            }
            "lineEnd" => {
                line_to = value.parse::<u32>().ok();
            }
            _ => {}
        }
    }

    // If line_from is set but line_to is not, set line_to to line_from for consistency
    if line_from.is_some() && line_to.is_none() {
        line_to = line_from;
    }

    (git_ref, path, line_from, line_to)
}

fn post_process_bitbucket_server_commits(
    url: &Url,
    _caps: &regex::Captures,
) -> (Option<String>, Option<String>, Option<u32>, Option<u32>) {
    // Extract path and line number from fragment if present
    // Format: #path/to/file.py?f=239 or #path/to/file.py?t=239
    let fragment = match url.fragment() {
        Some(f) => f,
        None => return (None, None, None, None),
    };

    // Parse the fragment as a URL by constructing a temporary URL
    let temp_url_str = format!("http://localhost/{}", fragment);
    let temp_url = match Url::parse(&temp_url_str) {
        Ok(u) => u,
        Err(_) => return (None, Some(fragment.to_string()), None, None),
    };

    // Extract path (strip leading /) and URL decode it
    let path = temp_url.path().strip_prefix('/').map(|p| {
        percent_encoding::percent_decode_str(p)
            .decode_utf8_lossy()
            .to_string()
    });

    // Parse query parameters for line number (f= or t=)
    let mut line_num = None;
    for (key, value) in temp_url.query_pairs() {
        match key.as_ref() {
            "f" | "t" => {
                line_num = value.parse::<u32>().ok();
                break;
            }
            _ => {}
        }
    }

    (None, path, line_num, line_num)
}

fn post_process_bitbucket_server(
    url: &Url,
    _caps: &regex::Captures,
) -> (Option<String>, Option<String>, Option<u32>, Option<u32>) {
    let git_ref = url
        .query_pairs()
        .find(|(k, _)| k == "at")
        .map(|(_, v)| v.strip_prefix("refs/heads/").unwrap_or(&v).to_string());
    // Extract line numbers using Bitbucket Server format
    let path = _caps.name("path").map(|m| m.as_str().to_string());
    let (line_from, line_to) = extract_lines_bitbucket_server(url, &path);
    (git_ref, None, line_from, line_to) // path is already in regex
}

fn post_process_github_gitlab_gitea(
    url: &Url,
    _caps: &regex::Captures,
) -> (Option<String>, Option<String>, Option<u32>, Option<u32>) {
    let (line_from, line_to) = extract_lines_github_style(url, &None);
    (None, None, line_from, line_to)
}

fn post_process_bitbucket_cloud(
    url: &Url,
    _caps: &regex::Captures,
) -> (Option<String>, Option<String>, Option<u32>, Option<u32>) {
    let (line_from, line_to) = extract_lines_bitbucket_cloud(url, &None);
    (None, None, line_from, line_to)
}

#[cfg(test)]
#[path = "parsed_repo_url_test.rs"]
mod tests;
