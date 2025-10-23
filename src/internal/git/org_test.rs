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
    // parsed org definition only
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
    // namespace only (colon shorthand) treated as owner/repo
    let org = mk_org("gitlab.com:group/sub1");
    assert_eq!(org.owner.as_deref(), Some("group"));
    assert_eq!(org.repo.as_deref(), Some("sub1"));

    // pinned repo via SSH/URL style also allowed
    let org = mk_org("https://gitlab.com/group/sub1/repo");
    assert_eq!(org.owner.as_deref(), Some("group/sub1"));
    assert_eq!(org.repo.as_deref(), Some("repo"));
}

#[test]
fn gitlab_partial_ssh_handles() {
    // owner only is interpreted as owner+repo in SSH scp form
    let org = mk_org("git@gitlab.com:group/sub");
    assert_eq!(org.owner.as_deref(), Some("group"));
    assert_eq!(org.repo.as_deref(), Some("sub"));
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

    // permissive form without _git
    let org = mk_org("https://dev.azure.com/Org/Project/Repo");
    assert_eq!(org.owner.as_deref(), Some("Org/Project"));
    assert_eq!(org.repo.as_deref(), Some("Repo"));
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

#[test]
fn get_repo_git_url_github() {
    // owner only -> matches any repo under owner
    let org = mk_org("https://github.com/xaf");
    let url = org.get_repo_git_url("omni").expect("should build url");
    assert_eq!(url, "https://github.com/xaf/omni");

    // pinned repo -> only that repo matches
    let org = mk_org("https://github.com/xaf/blah");
    assert!(org.get_repo_git_url("omni").is_none());
    assert_eq!(
        org.get_repo_git_url("blah").unwrap(),
        "https://github.com/xaf/blah"
    );
}

#[test]
fn get_repo_git_url_gitlab() {
    // owner only -> matches any repo under owner
    let org = mk_org("https://gitlab.com/group");
    let url = org.get_repo_git_url("repo").expect("should build url");
    assert_eq!(url, "https://gitlab.com/group/repo");

    // pinned repo -> only that repo matches
    let org = mk_org("https://gitlab.com/group/sub1/repo");
    assert!(org.get_repo_git_url("other").is_none());
    assert_eq!(
        org.get_repo_git_url("repo").unwrap(),
        "https://gitlab.com/group/sub1/repo"
    );
}

#[test]
fn get_repo_git_url_generic() {
    let org = mk_org("https://example.com/org");
    assert_eq!(
        org.get_repo_git_url("repo").unwrap(),
        "https://example.com/org/repo"
    );

    let org = mk_org("https://example.com/org/repo");
    assert!(org.get_repo_git_url("other").is_none());
    assert_eq!(
        org.get_repo_git_url("repo").unwrap(),
        "https://example.com/org/repo"
    );
}

#[test]
fn get_repo_git_url_azure() {
    // owner only -> builds with _git
    let org = mk_org("https://dev.azure.com/Org/Project");
    let url = org
        .get_repo_git_url("Repo")
        .expect("should build azure url");
    assert_eq!(url, "https://dev.azure.com/Org/Project/_git/Repo");

    // pinned repo -> must match
    let org = mk_org("https://dev.azure.com/Org/Project/_git/Repo");
    assert!(org.get_repo_git_url("Other").is_none());
    assert_eq!(
        org.get_repo_git_url("Repo").unwrap(),
        "https://dev.azure.com/Org/Project/_git/Repo"
    );
}

#[test]
fn get_repo_git_url_host_only_orgs() {
    // GitHub host-only, require OWNER/repo
    let org = mk_org("https://github.com");
    assert!(org.get_repo_git_url("repo").is_none());
    assert_eq!(
        org.get_repo_git_url("owner/repo").unwrap(),
        "https://github.com/owner/repo"
    );

    // Generic host-only
    let org = mk_org("https://example.com");
    assert!(org.get_repo_git_url("repo").is_none());
    assert_eq!(
        org.get_repo_git_url("org/repo").unwrap(),
        "https://example.com/org/repo"
    );

    // Azure host-only requires Org/Project/Repo
    let org = mk_org("https://dev.azure.com");
    assert!(org.get_repo_git_url("Repo").is_none());
    assert!(org.get_repo_git_url("Org/Project").is_none());
    assert_eq!(
        org.get_repo_git_url("Org/Project/Repo").unwrap(),
        "https://dev.azure.com/Org/Project/_git/Repo"
    );
}

#[test]
fn get_repo_git_url_owner_mismatch() {
    // When org has an owner, OWNER/repo matches; BLAH/repo does not
    let org = mk_org("https://github.com/OWNER");
    assert_eq!(
        org.get_repo_git_url("OWNER/repo").unwrap(),
        "https://github.com/OWNER/repo"
    );
    assert!(org.get_repo_git_url("BLAH/repo").is_none());

    let org = mk_org("https://gitlab.com/OWNER");
    assert_eq!(
        org.get_repo_git_url("OWNER/repo").unwrap(),
        "https://gitlab.com/OWNER/repo"
    );
    assert!(org.get_repo_git_url("BLAH/repo").is_none());

    let org = mk_org("https://dev.azure.com/Org/Project");
    assert_eq!(
        org.get_repo_git_url("Org/Project/Repo").unwrap(),
        "https://dev.azure.com/Org/Project/_git/Repo"
    );
    assert!(org.get_repo_git_url("Other/Project/Repo").is_none());
}
