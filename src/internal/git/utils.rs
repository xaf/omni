use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use git_url_parse::normalize_url;
use git_url_parse::GitUrl;
use lazy_static::lazy_static;
use tokio::runtime::Runtime;
use tokio::time::timeout;
use url::Url;

use crate::internal::commands::utils::abs_path;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::env::data_home;
use crate::internal::errors::GitUrlError;
use crate::internal::git_env;

lazy_static! {
    pub static ref PACKAGE_PATH: String = format!("{}/packages", data_home());
}

const PACKAGE_PATH_FORMAT: &str = "%{host}/%{org}/%{repo}";

pub fn package_root_path() -> String {
    PACKAGE_PATH.clone()
}

/* The safe_* helpers are to avoid the risk of Regular Expression Denial of Service (ReDos) attacks.
 * This is a similar issue to CVE-2023-32758 - https://github.com/advisories/GHSA-4xqq-73wg-5mjp
 * By setting a timeout, we prevent things from hanging indefinitely in case of such attack.
 */

static TIMEOUT_DURATION: Duration = Duration::from_secs(2);

async fn async_normalize_url(url: &str) -> Result<Url, GitUrlError> {
    Ok(normalize_url(url)?)
}

pub fn safe_normalize_url(url: &str) -> Result<Url, GitUrlError> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match timeout(TIMEOUT_DURATION, async_normalize_url(url)).await {
            Ok(result) => result,
            Err(_) => Err(GitUrlError::NormalizeTimeout),
        }
    })
}

async fn async_git_url_parse(url: &str) -> Result<GitUrl, GitUrlError> {
    Ok(GitUrl::parse(url)?)
}

pub fn safe_git_url_parse(url: &str) -> Result<GitUrl, GitUrlError> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match timeout(TIMEOUT_DURATION, async_git_url_parse(url)).await {
            Ok(result) => result,
            Err(_) => Err(GitUrlError::ParseTimeout),
        }
    })
}

pub fn id_from_git_url(url: &GitUrl) -> Option<String> {
    let url = url.clone();
    if let (Some(host), Some(owner), name) = (url.host, url.owner, url.name) {
        if !name.is_empty() {
            return Some(format!("{host}:{owner}/{name}"));
        }
    }
    None
}

pub fn full_git_url_parse(url: &str) -> Result<GitUrl, GitUrlError> {
    // let url = safe_normalize_url(url)?;
    // let git_url = safe_git_url_parse(url.as_str())?;
    let git_url = safe_git_url_parse(url)?;

    if git_url.scheme.to_string() == "file" {
        return Err(GitUrlError::UnsupportedScheme(git_url.scheme.to_string()));
    }
    if git_url.name.is_empty() {
        return Err(GitUrlError::MissingRepositoryName);
    }
    if git_url.owner.is_none() {
        return Err(GitUrlError::MissingRepositoryOwner);
    }
    if git_url.host.is_none() {
        return Err(GitUrlError::MissingRepositoryHost);
    }

    Ok(git_url)
}

pub fn format_path_with_template(worktree: &str, git_url: &GitUrl, path_format: &str) -> PathBuf {
    let git_url = git_url.clone();
    format_path_with_template_and_data(
        worktree,
        &git_url.host.unwrap(),
        &git_url.owner.unwrap(),
        &git_url.name,
        path_format,
    )
}

pub fn format_path_with_template_and_data(
    worktree: &str,
    host: &str,
    owner: &str,
    repo: &str,
    path_format: &str,
) -> PathBuf {
    // Create a path object
    let mut path = PathBuf::from(worktree.to_string());

    // Replace %{host}, #{owner}, and %{repo} with the actual values
    let path_format = path_format.to_string();
    let path_format = path_format.replace("%{host}", host);
    let path_format = path_format.replace("%{org}", owner);
    let path_format = path_format.replace("%{repo}", repo);

    // Split the path format into parts
    let path_format_parts = path_format.split('/');

    // Append each part to the path
    for part in path_format_parts {
        path.push(part);
    }

    // Return the path as a string
    path
}

pub fn package_path_from_handle(handle: &str) -> Option<PathBuf> {
    if let Ok(git_url) = full_git_url_parse(handle) {
        package_path_from_git_url(&git_url)
    } else {
        None
    }
}

pub fn package_path_from_git_url(git_url: &GitUrl) -> Option<PathBuf> {
    if git_url.scheme.to_string() == "file"
        || git_url.name.is_empty()
        || git_url.owner.is_none()
        || git_url.host.is_none()
    {
        return None;
    }

    let package_path =
        format_path_with_template(package_root_path().as_str(), git_url, PACKAGE_PATH_FORMAT);

    Some(package_path)
}

pub fn path_entry_config<T: AsRef<str>>(path: T) -> PathEntryConfig {
    let path: &str = path.as_ref();
    let git_env = git_env(path);

    let mut path_entry_config = PathEntryConfig {
        path: path.to_string(),
        package: None,
        full_path: path.to_string(),
    };

    if let (Some(id), Some(root)) = (git_env.id(), git_env.root()) {
        if PathBuf::from(root).starts_with(package_root_path()) {
            path_entry_config.package = Some(id.to_string());
            path_entry_config.path = PathBuf::from(path)
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
        }
    }

    path_entry_config
}

/// Checks if a given file path is ignored by Git according to .gitignore rules
///
/// # Arguments
/// * `file_path` - The path to the file to check
///
/// # Returns
/// * `Ok(bool)` - True if the file is ignored, false otherwise
/// * `Err(Box<dyn Error>)` - If there's an error accessing the repository or the path
///
/// # Example
/// ```rust
/// let is_ignored = is_path_gitignored("src/temp.log")?;
/// println!("Is file ignored: {}", is_ignored);
/// ```
pub fn is_path_gitignored<P: AsRef<Path>>(path: P) -> Result<bool, Box<dyn std::error::Error>> {
    let path = path.as_ref();

    // Find the directory to start the repository search from
    let search_dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        // If it's a file or doesn't exist, use its parent directory
        path.parent()
            .ok_or("Path has no parent directory")?
            .to_path_buf()
    };

    // Try to find the Git repository from the path's directory
    let repo = git2::Repository::discover(search_dir)?;

    // Get the absolute path
    let abs_path = abs_path(path);

    // Get the path relative to the repository root
    let repo_path = repo
        .workdir()
        .ok_or("Repository has no working directory")?;
    let rel_path = abs_path.strip_prefix(repo_path)?;

    // For directories, we check if a theoretical file inside would be ignored
    let check_path = if path.is_dir() {
        let uuid = uuid::Uuid::new_v4();
        rel_path.join(uuid.to_string())
    } else {
        rel_path.to_path_buf()
    };

    // Check if the path is ignored
    match repo.status_file(&check_path) {
        Ok(status) => Ok(status.contains(git2::Status::IGNORED)),
        Err(e) => {
            // If the path doesn't exist, we can still check if it would be ignored
            if e.code() == git2::ErrorCode::NotFound {
                Ok(repo.status_should_ignore(&check_path)?)
            } else {
                Err(e.into())
            }
        }
    }
}
