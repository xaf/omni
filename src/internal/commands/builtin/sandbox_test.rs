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
        let root = sandbox_root();
        fs::create_dir_all(&root).expect("create sandbox root");
        let name = command
            .generate_sandbox_name(&[], &root)
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
fn generate_sandbox_name_uses_dependency_prefix_when_possible() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let root = sandbox_root();
        fs::create_dir_all(&root).expect("create sandbox root");

        let dependencies = vec!["gopher@1.0.0".to_string()];
        let name = command
            .generate_sandbox_name(&dependencies, &root)
            .expect("should generate sandbox name");

        let first_word = name
            .split('-')
            .next()
            .expect("petname should have words")
            .to_ascii_lowercase();
        assert!(
            first_word.starts_with("go"),
            "first word {first_word} should reflect dependency prefix"
        );
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

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target");
        assert!(!allow_existing);
        assert_eq!(preferred_prefix.as_deref(), Some("demo"));

        let target = command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                preferred_prefix.as_deref(),
            )
            .expect("sandbox created");

        assert_eq!(target, sandbox_root().join("demo"));
        assert!(target.join(".omni/id").exists(), "expected .omni/id file");
        let id_contents = fs::read_to_string(target.join(".omni/id")).expect("id contents");
        assert!(
            id_contents.starts_with("demo:"),
            "workdir id should start with directory name: {id_contents}"
        );

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
        let dependencies = vec!["python".to_string()];

        let args = SandboxCommandArgs {
            path: None,
            name: Some("existing".to_string()),
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target");
        assert!(!allow_existing);

        command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                preferred_prefix.as_deref(),
            )
            .expect("first initialization should succeed");

        let err = command
            .determine_target_path(&args)
            .expect_err("second initialization should fail");
        assert!(
            err.contains("already exists"),
            "unexpected error message: {err}"
        );
    });
}

#[test]
fn initialize_autogenerated_name_creates_workdir() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let dependencies = vec!["python".to_string()];

        let args = SandboxCommandArgs {
            path: None,
            name: None,
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target");
        assert!(!allow_existing);
        let preferred = preferred_prefix.expect("auto generated name");

        assert!(target_path.starts_with(sandbox_root()));
        assert!(target_path.ends_with(&preferred));

        let target = command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                Some(&preferred),
            )
            .expect("initialized sandbox");

        assert!(target.join(".omni/id").exists());
        let id_contents = fs::read_to_string(target.join(".omni/id")).expect("id contents");
        assert!(
            id_contents.starts_with(&format!("{}:", preferred)),
            "workdir id should include generated name: {id_contents}"
        );
    });
}

#[test]
fn initialize_named_rejects_invalid_name() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let args = SandboxCommandArgs {
            path: None,
            name: Some("_bad-".to_string()),
            dependencies: vec![],
        };

        let err = command
            .determine_target_path(&args)
            .expect_err("should fail for invalid name");
        assert!(
            err.contains("sandbox name must start"),
            "unexpected error message: {err}"
        );
    });
}

#[test]
fn initialize_with_path_creates_sandbox() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let dependencies = vec!["python".to_string()];
        let sandbox_dir = sandbox_root().join("path_sandbox");
        fs::create_dir_all(&sandbox_dir).expect("create sandbox path");

        let args = SandboxCommandArgs {
            path: Some(sandbox_dir.clone()),
            name: None,
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target for path");
        assert!(allow_existing);

        command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                preferred_prefix.as_deref(),
            )
            .expect("initialize sandbox using path");

        assert!(sandbox_dir.join(".omni.yaml").exists());
        assert!(sandbox_dir.join(".omni/id").exists());
    });
}

#[test]
fn initialize_with_path_fails_if_omni_yaml_exists() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let dependencies = vec!["python".to_string()];
        let sandbox_dir = sandbox_root().join("path_with_yaml");
        fs::create_dir_all(&sandbox_dir).expect("create sandbox path");
        fs::write(sandbox_dir.join(".omni.yaml"), "existing").expect("write .omni.yaml");

        let args = SandboxCommandArgs {
            path: Some(sandbox_dir.clone()),
            name: None,
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target for path");
        assert!(allow_existing);

        let err = command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                preferred_prefix.as_deref(),
            )
            .expect_err("should fail when .omni.yaml exists");
        assert!(err.contains("already has an .omni.yaml"));
    });
}

#[test]
fn initialize_with_path_fails_if_workdir_exists() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let dependencies = vec!["python".to_string()];
        let sandbox_dir = sandbox_root().join("path_with_workdir");
        fs::create_dir_all(sandbox_dir.join(".omni")).expect("create .omni directory");
        fs::write(sandbox_dir.join(".omni/id"), "existing:id").expect("write .omni/id");

        let args = SandboxCommandArgs {
            path: Some(sandbox_dir.clone()),
            name: None,
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target for path");
        assert!(allow_existing);

        let err = command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                preferred_prefix.as_deref(),
            )
            .expect_err("should fail when .omni/id exists");
        assert!(err.contains("already contains a work directory"));
    });
}

#[test]
fn initialize_with_path_fails_if_git_repo_exists() {
    run_with_env(&[], || {
        reset_config();

        let command = SandboxCommand::new();
        let dependencies = vec!["python".to_string()];
        let sandbox_dir = sandbox_root().join("path_with_git");
        fs::create_dir_all(sandbox_dir.join(".git")).expect("create .git directory");

        let args = SandboxCommandArgs {
            path: Some(sandbox_dir.clone()),
            name: None,
            dependencies: dependencies.clone(),
        };

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target for path");
        assert!(allow_existing);

        let err = command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                preferred_prefix.as_deref(),
            )
            .expect_err("should fail when directory is git repo");
        assert!(err.contains("already contains a git repository"));
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

        let (target_path, allow_existing, preferred_prefix) = command
            .determine_target_path(&args)
            .expect("determine target");
        assert!(allow_existing);

        let target = command
            .initialize_at(
                &target_path,
                &dependencies,
                allow_existing,
                preferred_prefix.as_deref(),
            )
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
        let id_contents = fs::read_to_string(project_root.join(".omni/id")).expect("id");
        let prefix = id_contents.split(':').next().unwrap_or("");
        assert!(
            validate_sandbox_name(prefix).is_ok(),
            "workdir id should contain a valid prefix: {id_contents}"
        );
    });
}
