use super::ParsedRepoUrl;
use git_url_parse::GitUrl;

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

#[test]
fn inspect_git_url_with_paths_and_query() {
    // This test helps us understand what GitUrl::parse returns for URLs with branches and paths
    let test_cases = vec![
        "https://github.com/owner/repo/tree/main/src/lib.rs",
        "https://github.com/owner/repo/blob/feature-branch/README.md",
        "https://gitlab.com/group/repo/-/tree/main/src",
        "https://bitbucket.org/owner/repo/src/main/lib.rs",
        "https://dev.azure.com/org/project/_git/repo?path=/src/main.rs&version=GBmain",
    ];

    for url_str in test_cases {
        println!("\n=== Testing: {} ===", url_str);

        match GitUrl::parse(url_str) {
            Ok(url) => {
                println!("  scheme: {:?}", url.scheme());
                println!("  host: {:?}", url.host());
                println!("  port: {:?}", url.port());
                println!("  path: {:?}", url.path());
                println!("  user: {:?}", url.user());

                // Try to get provider info
                if let Ok(p) =
                    url.provider_info::<git_url_parse::types::provider::GenericProvider>()
                {
                    println!("  provider owner: {:?}", p.owner());
                    println!("  provider repo: {:?}", p.repo());
                }
            }
            Err(e) => {
                println!("  ERROR: {:?}", e);
            }
        }
    }

    // This test always passes - it's just for inspection
    assert!(true);
}

mod branch_path_extraction {
    use super::*;

    fn _test_branch_path_extraction(
        url: &str,
        expected_owner: Option<&str>,
        expected_name: &str,
        expected_branch: Option<&str>,
        expected_path: Option<&str>,
    ) {
        let p = ParsedRepoUrl::parse(url).expect("ok");
        assert_eq!(
            p.owner.as_deref(),
            expected_owner,
            "owner mismatch for URL: {}",
            url
        );
        assert_eq!(p.name, expected_name, "name mismatch for URL: {}", url);
        assert_eq!(
            p.branch.as_deref(),
            expected_branch,
            "branch mismatch for URL: {}",
            url
        );
        assert_eq!(
            p.path.as_deref(),
            expected_path,
            "path mismatch for URL: {}",
            url
        );
    }

    #[test]
    fn github_url_with_branch_and_path() {
        _test_branch_path_extraction(
            "https://github.com/owner/repo/tree/main/src/lib.rs",
            Some("owner"),
            "repo",
            Some("main"),
            Some("src/lib.rs"),
        );
    }

    #[test]
    fn github_url_with_branch_only() {
        _test_branch_path_extraction(
            "https://github.com/owner/repo/tree/feature-branch",
            Some("owner"),
            "repo",
            Some("feature-branch"),
            None,
        );
    }

    #[test]
    fn github_blob_url() {
        _test_branch_path_extraction(
            "https://github.com/owner/repo/blob/main/README.md",
            Some("owner"),
            "repo",
            Some("main"),
            Some("README.md"),
        );
    }

    #[test]
    fn gitlab_url_with_branch_and_path() {
        _test_branch_path_extraction(
            "https://gitlab.com/group/repo/-/tree/main/src/lib.rs",
            Some("group"),
            "repo",
            Some("main"),
            Some("src/lib.rs"),
        );
    }

    #[test]
    fn gitlab_subgroup_with_branch() {
        _test_branch_path_extraction(
            "https://gitlab.com/group/subgroup/repo/-/tree/develop",
            Some("group/subgroup"),
            "repo",
            Some("develop"),
            None,
        );
    }

    #[test]
    fn bitbucket_url() {
        _test_branch_path_extraction(
            "https://bitbucket.org/owner/repo/src/main/lib.rs",
            Some("owner"),
            "repo",
            Some("main"),
            Some("lib.rs"),
        );
    }

    #[test]
    fn azure_devops_url() {
        _test_branch_path_extraction(
            "https://dev.azure.com/org/project/_git/repo?path=/src/main.rs&version=GBmain",
            Some("org/project"),
            "repo",
            Some("main"),
            Some("src/main.rs"),
        );
    }

    #[test]
    fn github_url_without_branch() {
        _test_branch_path_extraction(
            "https://github.com/owner/repo",
            Some("owner"),
            "repo",
            None,
            None,
        );
    }

    #[test]
    fn ssh_url_no_branch_extraction() {
        _test_branch_path_extraction(
            "git@github.com:owner/repo.git",
            Some("owner"),
            "repo",
            None,
            None,
        );
    }

    // Gitea patterns
    #[test]
    fn gitea_src_branch() {
        _test_branch_path_extraction(
            "https://gitea.example.com/charlie/proj5/src/branch/main/README.md",
            Some("charlie"),
            "proj5",
            Some("main"),
            Some("README.md"),
        );
    }

    #[test]
    fn gitea_src_commit() {
        _test_branch_path_extraction(
            "https://gitea.example.com/charlie/proj5/src/commit/abc123def456/src/auth.go",
            Some("charlie"),
            "proj5",
            Some("abc123def456"),
            Some("src/auth.go"),
        );
    }

    #[test]
    fn gitea_raw_branch() {
        _test_branch_path_extraction(
            "https://gitea.example.com/charlie/proj5/raw/branch/main/README.md",
            Some("charlie"),
            "proj5",
            Some("main"),
            Some("README.md"),
        );
    }

    #[test]
    fn gitea_with_subgroups() {
        _test_branch_path_extraction(
            "https://gitea.example.com/bob/team2/proj6/src/branch/main/src/lib.rs",
            Some("bob/team2"),
            "proj6",
            Some("main"),
            Some("src/lib.rs"),
        );
    }

    // Gerrit gitiles /+/ pattern
    #[test]
    fn gerrit_gitiles_plus_pattern() {
        _test_branch_path_extraction(
            "https://gerrit.example.com/proj7/+/main/README.md",
            None,
            "proj7",
            Some("main"),
            Some("README.md"),
        );
    }

    #[test]
    fn gerrit_gitiles_refs_heads() {
        _test_branch_path_extraction(
            "https://gerrit.example.com/proj7/+/refs/heads/main/src/servlet/Main.java",
            None,
            "proj7",
            Some("main"),
            Some("src/servlet/Main.java"),
        );
    }

    #[test]
    fn gerrit_gitiles_commit() {
        _test_branch_path_extraction(
            "https://gerrit.example.com/proj7/+/abc123def456",
            None,
            "proj7",
            Some("abc123def456"),
            None,
        );
    }

    // SourceForge /ci/<ref>/tree/ pattern
    #[test]
    fn sourceforge_ci_tree() {
        _test_branch_path_extraction(
            "https://sourceforge.net/p/alice/proj8/ci/main/tree/src/main.c",
            Some("alice"),
            "proj8",
            Some("main"),
            Some("src/main.c"),
        );
    }

    #[test]
    fn sourceforge_ci_commit() {
        _test_branch_path_extraction(
            "https://sourceforge.net/p/alice/proj8/ci/abc123def456/",
            Some("alice"),
            "proj8",
            Some("abc123def456"),
            None,
        );
    }

    // Google Cloud colon separator
    #[test]
    fn google_cloud_colon_separator() {
        _test_branch_path_extraction(
            "https://source.cloud.google.com/bob/proj11/+/main:src/main.go",
            Some("bob"),
            "proj11",
            Some("main"),
            Some("src/main.go"),
        );
    }

    #[test]
    fn google_cloud_tree() {
        _test_branch_path_extraction(
            "https://source.cloud.google.com/bob/proj11/+/develop:src/",
            Some("bob"),
            "proj11",
            Some("develop"),
            Some("src/"),
        );
    }

    #[test]
    fn google_cloud_commit() {
        _test_branch_path_extraction(
            "https://source.cloud.google.com/bob/proj11/+/abc123def456",
            Some("bob"),
            "proj11",
            Some("abc123def456"),
            None,
        );
    }

    // AWS CodeCommit /browse/refs/heads/<ref>/--/ pattern
    #[test]
    fn aws_codecommit_browse() {
        _test_branch_path_extraction(
            "https://console.aws.amazon.com/codesuite/codecommit/repositories/proj9/browse/refs/heads/main/--/src/main.rs",
            None,
            "proj9",
            Some("main"),
            Some("src/main.rs"),
        );
    }

    #[test]
    fn aws_codecommit_commit() {
        _test_branch_path_extraction(
            "https://console.aws.amazon.com/codesuite/codecommit/repositories/proj9/commit/abc123def456",
            None,
            "proj9",
            Some("abc123def456"),
            None,
        );
    }

    // Bitbucket Server patterns
    #[test]
    fn bitbucket_server_browse_without_query() {
        _test_branch_path_extraction(
            "https://bitbucket.example.com/projects/proj1/repos/repo1/browse/src/main.rs",
            Some("proj1"),
            "repo1",
            None,
            Some("src/main.rs"),
        );
    }

    #[test]
    fn bitbucket_server_browse_with_query() {
        _test_branch_path_extraction(
            "https://bitbucket.example.com/projects/proj1/repos/repo1/browse/src/lib.rs?at=refs%2Fheads%2Ffeature-x",
            Some("proj1"),
            "repo1",
            Some("feature-x"),
            Some("src/lib.rs"),
        );
    }

    #[test]
    fn bitbucket_server_raw() {
        _test_branch_path_extraction(
            "https://bitbucket.example.com/projects/proj1/repos/repo1/raw/README.md?at=refs%2Fheads%2Fmain",
            Some("proj1"),
            "repo1",
            Some("main"),
            Some("README.md"),
        );
    }

    #[test]
    fn bitbucket_server_commits_with_fragment() {
        _test_branch_path_extraction(
            "https://bitbucket.example.com/projects/proj1/repos/repo1/commits/abc123def456#readme.md",
            Some("proj1"),
            "repo1",
            Some("abc123def456"),
            Some("readme.md"),
        );
    }

    // GitHub raw.githubusercontent.com
    #[test]
    fn github_raw_domain() {
        _test_branch_path_extraction(
            "https://raw.githubusercontent.com/alice/proj2/refs/heads/main/tests/test_config.rs",
            Some("alice"),
            "proj2",
            Some("main"),
            Some("tests/test_config.rs"),
        );
    }

    // GitLab with query parameters
    #[test]
    fn gitlab_tree_with_query() {
        _test_branch_path_extraction(
            "https://gitlab.com/bob/proj3/-/tree/feature-y?ref_type=heads",
            Some("bob"),
            "proj3",
            Some("feature-y"),
            None,
        );
    }

    #[test]
    fn gitlab_blob_with_query() {
        _test_branch_path_extraction(
            "https://gitlab.com/bob/proj3/-/blob/feature-y/tests/test_base.py?ref_type=heads",
            Some("bob"),
            "proj3",
            Some("feature-y"),
            Some("tests/test_base.py"),
        );
    }

    #[test]
    fn gitlab_raw() {
        _test_branch_path_extraction(
            "https://gitlab.com/bob/proj3/-/raw/abc123def456/src/main.py",
            Some("bob"),
            "proj3",
            Some("abc123def456"),
            Some("src/main.py"),
        );
    }

    // Azure DevOps with slashes in branch name
    #[test]
    fn azure_devops_branch_with_slashes() {
        _test_branch_path_extraction(
            "https://dev.azure.com/alice/proj10/_git/repo1?path=/README.md&version=GBfeature/new-api",
            Some("alice/proj10"),
            "repo1",
            Some("feature/new-api"),
            Some("README.md"),
        );
    }

    #[test]
    fn azure_devops_tree() {
        _test_branch_path_extraction(
            "https://dev.azure.com/alice/proj10/_git/repo1?path=/src&version=GBmain",
            Some("alice/proj10"),
            "repo1",
            Some("main"),
            Some("src"),
        );
    }
}
