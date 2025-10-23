use super::ParsedRepoUrl;

fn assert_parsed(
    input: &str,
    exp_scheme: Option<&str>,
    exp_host: Option<&str>,
    exp_owner: Option<&str>,
    exp_name: &str,
) {
    let p = ParsedRepoUrl::parse(input).expect("parse should succeed");
    assert_eq!(p.scheme.as_deref(), exp_scheme);
    assert_eq!(p.host.as_deref(), exp_host);
    assert_eq!(p.owner.as_deref(), exp_owner);
    assert_eq!(p.name.as_str(), exp_name);
}

fn assert_parsed_full(
    input: &str,
    exp_scheme: Option<&str>,
    exp_host: Option<&str>,
    exp_owner: Option<&str>,
    exp_name: &str,
    exp_user: Option<&str>,
    exp_password: Option<&str>,
    exp_port: Option<u16>,
) {
    let p = ParsedRepoUrl::parse(input).expect("parse should succeed");
    assert_eq!(p.scheme.as_deref(), exp_scheme);
    assert_eq!(p.host.as_deref(), exp_host);
    assert_eq!(p.owner.as_deref(), exp_owner);
    assert_eq!(p.name.as_str(), exp_name);
    assert_eq!(p.user.as_deref(), exp_user);
    assert_eq!(p.password.as_deref(), exp_password);
    assert_eq!(p.port, exp_port);
}

#[test]
fn github_https_and_ssh() {
    assert_parsed(
        "https://github.com/owner/repo.git",
        Some("https"),
        Some("github.com"),
        Some("owner"),
        "repo",
    );
    assert_parsed(
        "git@github.com:owner/repo.git",
        Some("ssh"),
        Some("github.com"),
        Some("owner"),
        "repo",
    );
}

#[test]
fn github_with_port_and_userinfo() {
    // userinfo + port
    assert_parsed_full(
        "https://user:token@github.com:443/owner/repo.git",
        Some("https"),
        Some("github.com"),
        Some("owner"),
        "repo",
        Some("user"),
        Some("token"),
        Some(443),
    );
}

#[test]
fn gitlab_subgroups_https_and_ssh() {
    assert_parsed(
        "https://gitlab.com/group/sub1/repo.git",
        Some("https"),
        Some("gitlab.com"),
        Some("group/sub1"),
        "repo",
    );
    assert_parsed(
        "git@gitlab.com:group/sub1/repo.git",
        Some("ssh"),
        Some("gitlab.com"),
        Some("group/sub1"),
        "repo",
    );
}

#[test]
fn gitlab_deep_subgroups() {
    assert_parsed(
        "https://gitlab.com/a/b/c/d/repo.git",
        Some("https"),
        Some("gitlab.com"),
        Some("a/b/c/d"),
        "repo",
    );
    assert_parsed(
        "git@gitlab.com:a/b/c/d/repo.git",
        Some("ssh"),
        Some("gitlab.com"),
        Some("a/b/c/d"),
        "repo",
    );
}

#[test]
fn gitlab_with_port_and_userinfo() {
    assert_parsed_full(
        "https://user:token@gitlab.com:8443/group/sub/repo.git",
        Some("https"),
        Some("gitlab.com"),
        Some("group/sub"),
        "repo",
        Some("user"),
        Some("token"),
        Some(8443),
    );
}

#[test]
fn azure_https_and_ssh() {
    // Canonical HTTPS form with _git
    assert_parsed(
        "https://dev.azure.com/Org/Project/_git/Repo",
        Some("https"),
        Some("dev.azure.com"),
        Some("Org/Project"),
        "Repo",
    );
    // SSH form with v3 prefix
    assert_parsed(
        "git@ssh.dev.azure.com:v3/Org/Project/Repo",
        Some("ssh"),
        Some("ssh.dev.azure.com"),
        Some("Org/Project"),
        "Repo",
    );
    // HTTPS without _git
    assert_parsed(
        "https://dev.azure.com/Org/Project/Repo",
        Some("https"),
        Some("dev.azure.com"),
        Some("Org/Project"),
        "Repo",
    );
}

#[test]
fn azure_with_userinfo_and_port_https() {
    assert_parsed_full(
        "https://user:token@dev.azure.com:443/Org/Project/_git/Repo",
        Some("https"),
        Some("dev.azure.com"),
        Some("Org/Project"),
        "Repo",
        Some("user"),
        Some("token"),
        Some(443),
    );
}

#[test]
fn generic_host_https_and_ssh() {
    assert_parsed(
        "https://example.com/org/repo",
        Some("https"),
        Some("example.com"),
        Some("org"),
        "repo",
    );
    assert_parsed(
        "git@example.com:org/repo",
        Some("ssh"),
        Some("example.com"),
        Some("org"),
        "repo",
    );
}

#[test]
fn negative_cases() {
    // Clearly invalid: missing path separator
    assert!(ParsedRepoUrl::parse("git@github.com").is_err());
    // Invalid scheme form
    assert!(ParsedRepoUrl::parse("://invalid").is_err());
}

#[test]
fn partial_cases_are_valid() {
    // Partial SSH (owner only) should parse but have empty repo name
    let p = ParsedRepoUrl::parse("git@github.com:owner").expect("ok");
    assert_eq!(p.owner.as_deref(), Some("owner"));
    assert_eq!(p.name, "");

    // Azure partial HTTPS (org/project) should parse with empty repo name
    let p = ParsedRepoUrl::parse("https://dev.azure.com/Org/Project").expect("ok");
    assert_eq!(p.owner.as_deref(), Some("Org/Project"));
    assert_eq!(p.name, "");

    // Azure partial SSH (v3/org/project) should parse with empty repo name
    let p = ParsedRepoUrl::parse("git@ssh.dev.azure.com:v3/Org/Project").expect("ok");
    assert_eq!(p.owner.as_deref(), Some("Org/Project"));
    assert_eq!(p.name, "");
}
