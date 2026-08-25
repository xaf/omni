use super::*;
use std::fs;
use std::io::Write;
use tempfile::tempdir;

fn create_test_file(dir: &std::path::Path, filename: &str, content: &str) -> PathBuf {
    let file_path = dir.join(filename);
    let mut file = fs::File::create(&file_path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file_path
}

mod detect_version_from_pyproject_toml {
    use super::*;

    // Helper function to create a temporary directory with a pyproject.toml file
    fn setup_test_dir(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let file_path = create_test_file(dir.path(), "pyproject.toml", content);
        (dir, file_path)
    }

    #[test]
    fn from_pep621_requires_python() {
        let pyproject_content = r#"
[project]
name = "my-package"
version = "0.1.0"
requires-python = ">=3.8, <4.0"
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, Some(">=3.8, <4.0".to_string()));
    }

    #[test]
    fn from_poetry_dependencies() {
        let pyproject_content = r#"
[tool.poetry]
name = "my-package"
version = "0.1.0"

[tool.poetry.dependencies]
python = "^3.9"
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, Some("^3.9".to_string()));
    }

    #[test]
    fn from_poetry_dependencies_with_table() {
        let pyproject_content = r#"
[tool.poetry]
name = "my-package"
version = "0.1.0"

[tool.poetry.dependencies]
python = { version = ">=3.10", optional = false }
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, Some(">=3.10".to_string()));
    }

    #[test]
    fn from_build_system_requires() {
        let pyproject_content = r#"
[build-system]
requires = ["setuptools>=42.0", "wheel", "python>=3.7"]
build-backend = "setuptools.build_meta"
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, Some("=3.7".to_string()));
    }

    #[test]
    fn no_python_requirement() {
        let pyproject_content = r#"
[build-system]
requires = ["setuptools>=42.0", "wheel"]
build-backend = "setuptools.build_meta"

[tool.black]
line-length = 88
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, None);
    }

    #[test]
    fn file_not_exists() {
        let dir = tempdir().unwrap();
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, None);
    }

    #[test]
    fn invalid_toml() {
        let pyproject_content = r#"
This is not valid TOML content
[project
requires-python = ">=3.8, <4.0"
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, None);
    }

    #[test]
    fn empty_file() {
        let pyproject_content = "";

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, None);
    }

    #[test]
    fn file_is_directory() {
        let dir = tempdir().unwrap();
        let pyproject_path = dir.path().join("pyproject.toml");
        fs::create_dir_all(&pyproject_path).unwrap();

        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, None);
    }

    #[test]
    fn python_string_without_version() {
        let pyproject_content = r#"
[tool.poetry.dependencies]
python = "not-a-version"
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, Some("not-a-version".to_string()));
    }

    #[test]
    fn python_requires_non_string() {
        let pyproject_content = r#"
[project]
name = "my-package"
version = "0.1.0"
requires-python = 3.8
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, None);
    }

    #[test]
    fn mixed_format_requirement() {
        let pyproject_content = r#"
[build-system]
requires = ["setuptools>=42.0", "wheel", "python_version>='3.8'"]
build-backend = "setuptools.build_meta"
"#;

        let (dir, _) = setup_test_dir(pyproject_content);
        let result =
            detect_version_from_pyproject_toml("python".to_string(), dir.path().to_path_buf());

        assert_eq!(result, Some("=3.8".to_string()));
    }
}

mod config_parsing {
    use compote::FromContextValue;

    use super::*;

    fn parse_python(yaml: &str) -> (UpConfigPython, compote::ErrorTracker) {
        let context = compote::Context::new(compote::Source::Programmatic, compote::Level::User);
        let mut config = compote::Config::default();
        config.load_yaml(yaml, context);
        let mut tracker = compote::ErrorTracker::new();
        let python = UpConfigPython::from_context_value(config.root(), &mut tracker).unwrap();
        (python, tracker)
    }

    #[test]
    fn scalar_version_uses_default_params_without_object_diagnostic() {
        let (python, tracker) = parse_python("3.12");

        assert_eq!(python.backend.requested_tool, "python");
        assert_eq!(python.backend.version, "3.12");
        assert!(matches!(python.params.pip, PipConfig::Auto));
        assert!(python.backend.retained_config_value().is_some());
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn object_form_retains_python_params_for_callbacks() {
        let (python, tracker) = parse_python("version: 3.11\npip: false\n");

        assert_eq!(python.backend.version, "3.11");
        assert!(matches!(python.params.pip, PipConfig::Disabled));
        let mut callback_tracker = compote::ErrorTracker::new();
        let callback_params = UpConfigPythonParams::from_context_value(
            python.backend.retained_config_value().unwrap(),
            &mut callback_tracker,
        )
        .unwrap();
        assert!(matches!(callback_params.pip, PipConfig::Disabled));
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
        assert!(
            callback_tracker.errors().is_empty(),
            "{:#?}",
            callback_tracker.errors()
        );
    }
}
