use super::*;
use crate::internal::config::parser::OrgConfig;

fn mk_org(handle: &str) -> Org {
    let cfg = OrgConfig {
        handle: handle.to_string(),
        trusted: true,
        worktree: None,
        repo_path_format: None,
    };
    Org::new(cfg).expect("Org::new should parse handle")
}

#[test]
fn github_partial_handles() {
    // host only
    let org = mk_org("https://github.com");
    assert!(org.owner.is_none());
    assert!(org.repo.is_none());

    // owner only (URL form)
    let org = mk_org("https://github.com/xaf");
    assert_eq!(org.owner.as_deref(), Some("xaf"));
    assert!(org.repo.is_none());

    // owner only (shorthand host:owner)
    let org = mk_org("github.com:xaf");
    assert_eq!(org.owner.as_deref(), Some("xaf"));
    assert!(org.repo.is_none());

    // owner+repo (shorthand)
    let org = mk_org("github.com:xaf/repo");
    assert_eq!(org.owner.as_deref(), Some("xaf"));
    assert_eq!(org.repo.as_deref(), Some("repo"));
    // org with repo pinned should not accept a different repo
    assert!(org.get_repo_git_url("other").is_none());
    assert!(org.get_repo_git_url("repo").is_some());
}

#[test]
fn github_partial_ssh_handles() {
    // owner only
    let org = mk_org("git@github.com:owner");
    assert_eq!(org.owner.as_deref(), Some("owner"));
    assert!(org.repo.is_none());
    // owner + repo
    let org = mk_org("git@github.com:owner/repo.git");
    assert_eq!(org.owner.as_deref(), Some("owner"));
    assert_eq!(org.repo.as_deref(), Some("repo"));
}

#[test]
fn gitlab_namespace_parsing() {
    // namespace only
    let org = mk_org("gitlab.com:group/sub1");
    assert_eq!(org.owner.as_deref(), Some("group/sub1"));
    assert!(org.repo.is_none());

    // pinned repo via SSH/URL style also allowed
    let org = mk_org("https://gitlab.com/group/sub1/repo");
    assert_eq!(org.owner.as_deref(), Some("group/sub1"));
    assert_eq!(org.repo.as_deref(), Some("repo"));
    assert!(org.get_repo_git_url("repo").is_some());
    assert!(org.get_repo_git_url("other").is_none());
}

#[test]
fn gitlab_partial_ssh_handles() {
    // owner only
    let org = mk_org("git@gitlab.com:group/sub");
    assert_eq!(org.owner.as_deref(), Some("group/sub"));
    assert!(org.repo.is_none());
    // owner + repo
    let org = mk_org("git@gitlab.com:group/sub/repo.git");
    assert_eq!(org.owner.as_deref(), Some("group/sub"));
    assert_eq!(org.repo.as_deref(), Some("repo"));
}

#[test]
fn generic_host_parsing() {
    let org = mk_org("https://example.com/org");
    assert_eq!(org.owner.as_deref(), Some("org"));
    assert!(org.repo.is_none());
    let org = mk_org("https://example.com/org/repo");
    assert_eq!(org.owner.as_deref(), Some("org"));
    assert_eq!(org.repo.as_deref(), Some("repo"));
    assert!(org.get_repo_git_url("repo").is_some());
}

#[test]
fn azure_owner_repo_parsing() {
    // org only
    let org = mk_org("https://dev.azure.com/Org");
    assert_eq!(org.owner.as_deref(), Some("Org"));
    assert!(org.repo.is_none());

    // org + project
    let org = mk_org("https://dev.azure.com/Org/Project");
    assert_eq!(org.owner.as_deref(), Some("Org/Project"));
    assert!(org.repo.is_none());

    // org + project + _git
    let org = mk_org("https://dev.azure.com/Org/Project/_git");
    assert_eq!(org.owner.as_deref(), Some("Org/Project"));
    assert!(org.repo.is_none());

    // full https form
    let org = mk_org("https://dev.azure.com/Org/Project/_git/Repo");
    assert_eq!(org.owner.as_deref(), Some("Org/Project"));
    assert_eq!(org.repo.as_deref(), Some("Repo"));
    assert!(org.get_repo_git_url("Repo").is_some());
    assert!(org.get_repo_git_url("Other").is_none());

    // permissive form without _git
    let org = mk_org("https://dev.azure.com/Org/Project/Repo");
    assert_eq!(org.owner.as_deref(), Some("Org/Project"));
    assert_eq!(org.repo.as_deref(), Some("Repo"));
    assert!(org.get_repo_git_url("Repo").is_some());
}

#[test]
fn azure_ssh_owner_only_and_repo() {
    // owner only via SSH
    let org = mk_org("git@ssh.dev.azure.com:v3/Org/Project");
    assert_eq!(org.owner.as_deref(), Some("Org/Project"));
    assert!(org.repo.is_none());

    // owner + repo via SSH
    let org = mk_org("git@ssh.dev.azure.com:v3/Org/Project/Repo");
    assert_eq!(org.owner.as_deref(), Some("Org/Project"));
    assert_eq!(org.repo.as_deref(), Some("Repo"));
}

#[test]
fn generic_partial_ssh_handles() {
    // owner only
    let org = mk_org("git@example.com:org");
    assert_eq!(org.owner.as_deref(), Some("org"));
    assert!(org.repo.is_none());
    // owner + repo
    let org = mk_org("git@example.com:org/repo");
    assert_eq!(org.owner.as_deref(), Some("org"));
    assert_eq!(org.repo.as_deref(), Some("repo"));
}
