use super::*;

use std::fs;
use std::path::{Path, PathBuf};

use crate::internal::config::{config, flush_config};
use crate::internal::testutils::run_with_env;

struct DirGuard {
    original: PathBuf,
}

impl DirGuard {
    fn change_to(target: &Path) -> Self {
        let original = std::env::current_dir().expect("failed to get current directory");
        std::env::set_current_dir(target).expect("failed to change directory");
        Self { original }
    }
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.original).expect("failed to restore working directory");
    }
}

fn sandbox_root() -> PathBuf {
    PathBuf::from(config(".").sandbox())
}

fn reset_config() {
    flush_config("/");
    flush_config(".");
}

#[test]
fn resolve_target_validates_name_and_creates_root() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();

        let target = command
            .resolve_target("example")
            .expect("valid sandbox name");
        assert_eq!(target, sandbox_root().join("example"));

        assert!(command.resolve_target("/absolute").is_err());
        assert!(command.resolve_target("../outside").is_err());
        assert!(command.resolve_target("").is_err());
    });
}

#[test]
fn generate_sandbox_name_produces_safe_value() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let name = command
            .generate_sandbox_name()
            .expect("should generate sandbox name");

        assert!(!name.is_empty(), "generated name should not be empty");
        assert!(
            !name.contains('/'),
            "generated name should not contain path separators: {name}"
        );
        assert!(
            !name.contains(' '),
            "generated name should not contain whitespace: {name}"
        );

        command
            .resolve_target(&name)
            .expect("generated name should be valid");
    });
}

#[test]
fn initialize_named_creates_config_and_id() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let dependencies = vec!["python".to_string(), "go@1.21.0".to_string()];

        let args = SandboxCommandArgs {
            path: None,
            name: Some("demo".to_string()),
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing) = command
            .determine_target_path(&args)
            .expect("determine target");
        assert!(!allow_existing);

        let target = command
            .initialize_at(&target_path, &dependencies, allow_existing)
            .expect("sandbox created");

        assert_eq!(target, sandbox_root().join("demo"));
        assert!(target.join(".omni/id").exists(), "expected .omni/id file");

        let config_path = target.join(".omni.yaml");
        let contents = fs::read_to_string(&config_path).expect("config contents");
        assert!(
            contents.contains("  - python\n"),
            "expected python dependency in config"
        );
        assert!(
            contents.contains("  - go@1.21.0\n"),
            "expected go dependency in config"
        );
    });
}

#[test]
fn initialize_named_fails_when_directory_exists() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let existing = sandbox_root().join("existing");
        fs::create_dir_all(&existing).expect("create existing sandbox");

        let args = SandboxCommandArgs {
            path: None,
            name: Some("existing".to_string()),
            dependencies: vec![],
        };

        let err = command
            .determine_target_path(&args)
            .expect_err("should fail for existing directory");
        assert!(
            err.contains("already exists"),
            "unexpected error message: {err}"
        );
    });
}

#[test]
fn initialize_existing_configures_current_directory() {
    run_with_env(&[], || {
        reset_config();

        let raw_project_root = PathBuf::from(std::env::var("HOME").unwrap()).join("projects/app");
        fs::create_dir_all(&raw_project_root).expect("create project directory");
        let project_root = fs::canonicalize(&raw_project_root).expect("canonical project path");

        let _guard = DirGuard::change_to(&project_root);

        let command = SandboxCommand::new();
        let dependencies = vec!["nodejs@20.0.0".to_string()];

        let args = SandboxCommandArgs {
            path: Some(project_root.clone()),
            name: None,
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing) = command
            .determine_target_path(&args)
            .expect("determine target");
        assert!(allow_existing);

        let target = command
            .initialize_at(&target_path, &dependencies, allow_existing)
            .expect("initialized current directory");
        let canonical_target =
            fs::canonicalize(&target).expect("canonicalized sandbox directory path");
        assert_eq!(canonical_target, project_root);

        let config_path = project_root.join(".omni.yaml");
        let contents = fs::read_to_string(&config_path).expect("config contents");
        assert!(
            contents.contains("  - nodejs@20.0.0\n"),
            "expected dependency entry in config"
        );
        assert!(
            project_root.join(".omni/id").exists(),
            "expected .omni/id to be created"
        );
    });
}
