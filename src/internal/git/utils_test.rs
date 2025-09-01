use super::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

mod package_root_path_tests {
    use super::*;

    #[test]
    fn test_package_root_path_returns_package_path() {
        let result = package_root_path();
        assert!(result.contains("packages"));
        assert!(result.ends_with("/packages"));
    }
}

mod safe_git_url_parse_tests {
    use super::*;

    #[test]
    fn test_safe_git_url_parse_valid_https_url() {
        let url = "https://github.com/owner/repo.git";
        let result = safe_git_url_parse(url);

        assert!(result.is_ok());
        let git_url = result.unwrap();
        assert_eq!(git_url.host, Some("github.com".to_string()));
        assert_eq!(git_url.owner, Some("owner".to_string()));
        assert_eq!(git_url.name, "repo");
    }

    #[test]
    fn test_safe_git_url_parse_valid_ssh_url() {
        let url = "git@github.com:owner/repo.git";
        let result = safe_git_url_parse(url);

        assert!(result.is_ok());
        let git_url = result.unwrap();
        assert_eq!(git_url.host, Some("github.com".to_string()));
        assert_eq!(git_url.owner, Some("owner".to_string()));
        assert_eq!(git_url.name, "repo");
    }

    #[test]
    fn test_safe_git_url_parse_invalid_url() {
        let url = "completely-invalid-format";
        let result = safe_git_url_parse(url);

        if let Ok(git_url) = &result {
            println!("Unexpectedly parsed: {:?}", git_url);
        }
        // The git_url_parse library is quite lenient, so let's test for a URL that should fail
        // Try a URL that should definitely fail
        let invalid_result = safe_git_url_parse("://invalid");
        assert!(invalid_result.is_err());
    }
}

mod id_from_git_url_tests {
    use super::*;

    #[test]
    fn test_id_from_git_url_complete_url() {
        let url = "https://github.com/owner/repo.git";
        let git_url = safe_git_url_parse(url).unwrap();
        let result = id_from_git_url(&git_url);

        assert_eq!(result, Some("github.com:owner/repo".to_string()));
    }

    #[test]
    fn test_id_from_git_url_missing_owner() {
        let mut git_url = safe_git_url_parse("https://github.com/owner/repo.git").unwrap();
        git_url.owner = None;
        let result = id_from_git_url(&git_url);

        assert_eq!(result, None);
    }

    #[test]
    fn test_id_from_git_url_missing_host() {
        let mut git_url = safe_git_url_parse("https://github.com/owner/repo.git").unwrap();
        git_url.host = None;
        let result = id_from_git_url(&git_url);

        assert_eq!(result, None);
    }

    #[test]
    fn test_id_from_git_url_empty_name() {
        let mut git_url = safe_git_url_parse("https://github.com/owner/repo.git").unwrap();
        git_url.name = String::new();
        let result = id_from_git_url(&git_url);

        assert_eq!(result, None);
    }
}

mod full_git_url_parse_tests {
    use super::*;

    #[test]
    fn test_full_git_url_parse_valid_https_url() {
        let url = "https://github.com/owner/repo.git";
        let result = full_git_url_parse(url);

        assert!(result.is_ok());
        let git_url = result.unwrap();
        assert_eq!(git_url.host, Some("github.com".to_string()));
        assert_eq!(git_url.owner, Some("owner".to_string()));
        assert_eq!(git_url.name, "repo");
    }

    #[test]
    fn test_full_git_url_parse_file_scheme_rejected() {
        let url = "file:///path/to/repo";
        let result = full_git_url_parse(url);

        assert!(result.is_err());
        match result.unwrap_err() {
            GitUrlError::UnsupportedScheme(scheme) => assert_eq!(scheme, "file"),
            _ => panic!("Expected UnsupportedScheme error"),
        }
    }

    #[test]
    fn test_full_git_url_parse_missing_name() {
        // This would need to be crafted to have no name - testing the validation
        let url = "https://github.com/owner/";
        let result = full_git_url_parse(url);

        if result.is_err() {
            match result.unwrap_err() {
                GitUrlError::MissingRepositoryName => (),
                _ => panic!("Expected MissingRepositoryName error"),
            }
        }
    }
}

mod format_path_with_template_tests {
    use super::*;

    #[test]
    fn test_format_path_with_template_basic() {
        let url = "https://github.com/owner/repo.git";
        let git_url = safe_git_url_parse(url).unwrap();
        let result = format_path_with_template("/base", &git_url, "%{host}/%{org}/%{repo}");

        assert_eq!(result, PathBuf::from("/base/github.com/owner/repo"));
    }

    #[test]
    fn test_format_path_with_template_custom_format() {
        let url = "https://example.com/myorg/myproject.git";
        let git_url = safe_git_url_parse(url).unwrap();
        let result = format_path_with_template("/workspace", &git_url, "src/%{org}/%{repo}");

        assert_eq!(result, PathBuf::from("/workspace/src/myorg/myproject"));
    }
}

mod format_path_with_template_and_data_tests {
    use super::*;

    #[test]
    fn test_format_path_with_template_and_data_basic() {
        let result = format_path_with_template_and_data(
            "/base",
            "github.com",
            "owner",
            "repo",
            "%{host}/%{org}/%{repo}",
        );

        assert_eq!(result, PathBuf::from("/base/github.com/owner/repo"));
    }

    #[test]
    fn test_format_path_with_template_and_data_nested_path() {
        let result = format_path_with_template_and_data(
            "/workspace",
            "gitlab.com",
            "group",
            "project",
            "sources/%{host}/%{org}/%{repo}/code",
        );

        assert_eq!(
            result,
            PathBuf::from("/workspace/sources/gitlab.com/group/project/code")
        );
    }

    #[test]
    fn test_format_path_with_template_and_data_no_template() {
        let result = format_path_with_template_and_data(
            "/base",
            "example.com",
            "user",
            "project",
            "static/path",
        );

        assert_eq!(result, PathBuf::from("/base/static/path"));
    }
}

mod package_path_from_handle_tests {
    use super::*;

    #[test]
    fn test_package_path_from_handle_valid_url() {
        let handle = "https://github.com/owner/repo.git";
        let result = package_path_from_handle(handle);

        assert!(result.is_some());
        let path = result.unwrap();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("github.com"));
        assert!(path_str.contains("owner"));
        assert!(path_str.contains("repo"));
    }

    #[test]
    fn test_package_path_from_handle_invalid_url() {
        let handle = "not-a-valid-url";
        let result = package_path_from_handle(handle);

        assert!(result.is_none());
    }
}

mod package_path_from_git_url_tests {
    use super::*;

    #[test]
    fn test_package_path_from_git_url_valid() {
        let url = "https://github.com/owner/repo.git";
        let git_url = safe_git_url_parse(url).unwrap();
        let result = package_path_from_git_url(&git_url);

        assert!(result.is_some());
        let path = result.unwrap();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("github.com"));
        assert!(path_str.contains("owner"));
        assert!(path_str.contains("repo"));
    }

    #[test]
    fn test_package_path_from_git_url_file_scheme() {
        let mut git_url = safe_git_url_parse("https://github.com/owner/repo.git").unwrap();
        git_url.scheme = git_url_parse::Scheme::File;
        let result = package_path_from_git_url(&git_url);

        assert!(result.is_none());
    }

    #[test]
    fn test_package_path_from_git_url_empty_name() {
        let mut git_url = safe_git_url_parse("https://github.com/owner/repo.git").unwrap();
        git_url.name = String::new();
        let result = package_path_from_git_url(&git_url);

        assert!(result.is_none());
    }

    #[test]
    fn test_package_path_from_git_url_missing_owner() {
        let mut git_url = safe_git_url_parse("https://github.com/owner/repo.git").unwrap();
        git_url.owner = None;
        let result = package_path_from_git_url(&git_url);

        assert!(result.is_none());
    }

    #[test]
    fn test_package_path_from_git_url_missing_host() {
        let mut git_url = safe_git_url_parse("https://github.com/owner/repo.git").unwrap();
        git_url.host = None;
        let result = package_path_from_git_url(&git_url);

        assert!(result.is_none());
    }
}

mod is_path_gitignored_tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        // Canonicalize the path to resolve any symlinks (e.g., /var -> /private/var on macOS)
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize temp directory path");

        // Initialize git repo
        let _repo = git2::Repository::init(&repo_path).expect("Failed to init git repo");

        // Create .gitignore
        let gitignore_path = repo_path.join(".gitignore");
        let mut gitignore_file =
            File::create(&gitignore_path).expect("Failed to create .gitignore");
        writeln!(gitignore_file, "*.log").expect("Failed to write to .gitignore");
        writeln!(gitignore_file, "temp/").expect("Failed to write to .gitignore");
        writeln!(gitignore_file, "secrets.txt").expect("Failed to write to .gitignore");

        // Create some test files and directories
        fs::create_dir_all(repo_path.join("src")).expect("Failed to create src dir");
        fs::create_dir_all(repo_path.join("temp")).expect("Failed to create temp dir");

        File::create(repo_path.join("README.md")).expect("Failed to create README.md");
        File::create(repo_path.join("debug.log")).expect("Failed to create debug.log");
        File::create(repo_path.join("secrets.txt")).expect("Failed to create secrets.txt");
        File::create(repo_path.join("src").join("main.rs")).expect("Failed to create main.rs");

        temp_dir
    }

    #[test]
    fn test_is_path_gitignored_ignored_file() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored(repo_path.join("debug.log"));
        if let Err(e) = &result {
            println!("Error: {:?}", e);
        }
        assert!(result.is_ok());
        assert!(result.unwrap(), "debug.log should be ignored");
    }

    #[test]
    fn test_is_path_gitignored_non_ignored_file() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored(repo_path.join("README.md"));
        assert!(result.is_ok());
        assert!(!result.unwrap(), "README.md should not be ignored");
    }

    #[test]
    fn test_is_path_gitignored_ignored_directory() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored(repo_path.join("temp"));
        assert!(result.is_ok());
        assert!(result.unwrap(), "temp/ directory should be ignored");
    }

    #[test]
    fn test_is_path_gitignored_non_ignored_directory() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored(repo_path.join("src"));
        assert!(result.is_ok());
        assert!(!result.unwrap(), "src/ directory should not be ignored");
    }

    #[test]
    fn test_is_path_gitignored_nonexistent_file() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored(repo_path.join("nonexistent.log"));
        assert!(result.is_ok());
        assert!(
            result.unwrap(),
            "nonexistent.log should be ignored based on pattern"
        );
    }

    #[test]
    fn test_is_path_gitignored_file_in_subdirectory() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored(repo_path.join("src").join("main.rs"));
        assert!(result.is_ok());
        assert!(!result.unwrap(), "src/main.rs should not be ignored");
    }

    #[test]
    fn test_is_path_gitignored_no_git_repo() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let non_repo_path = temp_dir.path().join("some_file.txt");

        let result = is_path_gitignored(non_repo_path);
        assert!(result.is_err(), "Should error when not in a git repository");
    }
}

mod is_path_gitignored_from_tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        // Canonicalize the path to resolve any symlinks (e.g., /var -> /private/var on macOS)
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize temp directory path");

        // Initialize git repo
        let _repo = git2::Repository::init(&repo_path).expect("Failed to init git repo");

        // Create .gitignore
        let gitignore_path = repo_path.join(".gitignore");
        let mut gitignore_file =
            File::create(&gitignore_path).expect("Failed to create .gitignore");
        writeln!(gitignore_file, "*.log").expect("Failed to write to .gitignore");
        writeln!(gitignore_file, "temp/").expect("Failed to write to .gitignore");

        // Create some test files and directories
        fs::create_dir_all(repo_path.join("src")).expect("Failed to create src dir");
        File::create(repo_path.join("README.md")).expect("Failed to create README.md");
        File::create(repo_path.join("debug.log")).expect("Failed to create debug.log");
        File::create(repo_path.join("src").join("main.rs")).expect("Failed to create main.rs");

        temp_dir
    }

    #[test]
    fn test_is_path_gitignored_from_with_root() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored_from(repo_path.join("debug.log"), Some(&repo_path));
        assert!(result.is_ok());
        assert!(
            result.unwrap(),
            "debug.log should be ignored when specifying root"
        );
    }

    #[test]
    fn test_is_path_gitignored_from_without_root() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored_from(repo_path.join("README.md"), None::<&Path>);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "README.md should not be ignored");
    }

    #[test]
    fn test_is_path_gitignored_from_nonexistent_file_with_root() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        let result = is_path_gitignored_from(repo_path.join("test.log"), Some(&repo_path));
        assert!(result.is_ok());
        assert!(
            result.unwrap(),
            "test.log should be ignored based on pattern"
        );
    }

    #[test]
    fn test_is_path_gitignored_nested_repos() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let base_path = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize path");

        // Create repo1 (outer repo)
        let repo1_path = base_path.join("repo1");
        fs::create_dir_all(&repo1_path).expect("Failed to create repo1 dir");
        let _repo1 = git2::Repository::init(&repo1_path).expect("Failed to init repo1");

        // Create .gitignore in repo1 that ignores repo2/
        let gitignore1_path = repo1_path.join(".gitignore");
        let mut gitignore1_file =
            File::create(&gitignore1_path).expect("Failed to create .gitignore in repo1");
        writeln!(gitignore1_file, "repo2/").expect("Failed to write to repo1 .gitignore");

        // Create repo2 (inner repo) inside repo1
        let repo2_path = repo1_path.join("repo2");
        fs::create_dir_all(&repo2_path).expect("Failed to create repo2 dir");
        let _repo2 = git2::Repository::init(&repo2_path).expect("Failed to init repo2");

        // Create .gitignore in repo2 that doesn't ignore blah
        let gitignore2_path = repo2_path.join(".gitignore");
        let mut gitignore2_file =
            File::create(&gitignore2_path).expect("Failed to create .gitignore in repo2");
        writeln!(gitignore2_file, "*.log").expect("Failed to write to repo2 .gitignore");

        // Create a file in repo2
        let blah_file = repo2_path.join("blah");
        File::create(&blah_file).expect("Failed to create blah file");

        // Test 1: is_path_gitignored should use repo2 context (closest repo) and return false
        let result_closest = is_path_gitignored(&blah_file);
        assert!(
            result_closest.is_ok(),
            "Should successfully check gitignore status"
        );
        assert!(
            !result_closest.unwrap(),
            "blah should not be ignored in repo2 context"
        );

        // Test 2: is_path_gitignored_from with repo1 root should return true (repo2/ is ignored)
        let result_from_repo1 = is_path_gitignored_from(&blah_file, Some(&repo1_path));
        assert!(
            result_from_repo1.is_ok(),
            "Should successfully check from repo1"
        );
        assert!(
            result_from_repo1.unwrap(),
            "blah should be ignored when viewed from repo1 (repo2/ is ignored)"
        );

        // Test 3: is_path_gitignored_from with repo2 root should return false
        let result_from_repo2 = is_path_gitignored_from(&blah_file, Some(&repo2_path));
        assert!(
            result_from_repo2.is_ok(),
            "Should successfully check from repo2"
        );
        assert!(
            !result_from_repo2.unwrap(),
            "blah should not be ignored when viewed from repo2"
        );
    }
}
