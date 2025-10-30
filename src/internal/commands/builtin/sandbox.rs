use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::exit;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::utils::omni_cmd_always;
use crate::internal::commands::utils::validate_sandbox_name;
use crate::internal::commands::Command;
use crate::internal::config::config;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::omni_cmd_file;
use crate::internal::init_workdir;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir::add_trust;
use crate::omni_error;
use crate::omni_info;
use petname::{Generator, Petnames};
use shell_escape::escape;
use std::env::current_exe;

#[derive(Debug, Clone)]
struct SandboxCommandArgs {
    path: Option<PathBuf>,
    name: Option<String>,
    dependencies: Vec<String>,
}

impl From<BTreeMap<String, ParseArgsValue>> for SandboxCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let path = match args.get("path") {
            Some(ParseArgsValue::SingleString(Some(value))) => {
                if value.trim().is_empty() {
                    None
                } else {
                    Some(PathBuf::from(value))
                }
            }
            _ => None,
        };

        let name = match args.get("name") {
            Some(ParseArgsValue::SingleString(Some(value))) => {
                if value.trim().is_empty() {
                    None
                } else {
                    Some(value.clone())
                }
            }
            _ => None,
        };

        let dependencies = match args.get("dependencies") {
            Some(ParseArgsValue::ManyString(values)) => {
                values.iter().flat_map(|item| item.clone()).collect()
            }
            _ => vec![],
        };

        Self {
            path,
            name,
            dependencies,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SandboxCommand;

impl SandboxCommand {
    pub fn new() -> Self {
        Self
    }

    fn sandbox_root(&self) -> PathBuf {
        PathBuf::from(config(".").sandbox())
    }

    fn resolve_target(&self, name: &str) -> Result<PathBuf, String> {
        validate_sandbox_name(name)?;

        let path = Path::new(name);
        if path.is_absolute() {
            return Err("sandbox name must be relative".to_string());
        }

        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err("sandbox name cannot navigate outside the sandbox root".to_string());
        }

        let root = self.sandbox_root();

        if let Err(err) = fs::create_dir_all(&root) {
            return Err(format!(
                "failed to create sandbox root '{}': {}",
                root.display(),
                err
            ));
        }

        Ok(root.join(path))
    }

    fn generate_sandbox_name(
        &self,
        dependencies: &[String],
        root: &Path,
    ) -> Result<String, String> {
        let prefixes: Vec<String> = dependencies
            .iter()
            .filter_map(|dep| Self::dependency_prefix(dep))
            .collect();

        for prefix in &prefixes {
            if let Some(name) = Self::generate_name_with_prefix(prefix, root) {
                return Ok(name);
            }
        }

        for _ in 0..1000 {
            if let Some(name) = Petnames::default().generate_one(3, "-") {
                if !root.join(&name).exists() {
                    return Ok(name);
                }
            } else {
                break;
            }
        }

        Err("failed to generate sandbox name".to_string())
    }

    fn dependency_prefix(dep: &str) -> Option<String> {
        let base = dep.split('@').next().unwrap_or(dep);
        let cleaned: String = base
            .chars()
            .take_while(|ch| ch.is_ascii_alphabetic())
            .collect();

        if cleaned.is_empty() {
            return None;
        }

        let cleaned = cleaned.to_ascii_lowercase();

        if cleaned.len() == 1 {
            return Some(cleaned);
        }

        let mut prefix = String::new();
        for ch in cleaned.chars() {
            prefix.push(ch);
            if prefix.len() >= 4 || matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u' | 'y') {
                break;
            }
        }

        Some(prefix)
    }

    fn generate_name_with_prefix(prefix: &str, root: &Path) -> Option<String> {
        let prefix = prefix.to_ascii_lowercase();
        let mut generator = Petnames::default();
        generator
            .adverbs
            .to_mut()
            .retain(|word| word.starts_with(&prefix));

        for _ in 0..10 {
            let name = generator.generate_one(3, "-")?;
            let parts: Vec<_> = name.split('-').collect();
            if parts.len() != 3 {
                break;
            }

            let path = root.join(&name);
            if !path.exists() {
                return Some(name);
            }
        }

        None
    }

    fn write_config(&self, target: &Path, dependencies: &[String]) -> Result<(), String> {
        let config_path = target.join(".omni.yaml");

        if config_path.exists() {
            return Err(format!(
                "configuration file '{}' already exists",
                config_path.display()
            ));
        }

        let mut contents = String::new();
        contents.push_str("# Generated by omni sandbox\n");
        contents.push_str("up:\n");
        if dependencies.is_empty() {
            contents.push_str("  # Add your dependencies here, for example:\n");
            contents.push_str("  # - go\n");
            contents.push_str("  # - python\n");
        } else {
            for dependency in dependencies {
                contents.push_str(&format!("  - {dependency}\n"));
            }
        }
        if !contents.ends_with('\n') {
            contents.push('\n');
        }

        fs::write(&config_path, contents).map_err(|err| {
            format!(
                "failed to write configuration file '{}': {}",
                config_path.display(),
                err
            )
        })
    }

    fn determine_target_path(
        &self,
        args: &SandboxCommandArgs,
    ) -> Result<(PathBuf, bool, Option<String>), String> {
        if let Some(path) = &args.path {
            let generated = self
                .generate_sandbox_name(&args.dependencies, &self.sandbox_root())
                .ok();
            return Ok((path.clone(), true, generated));
        }

        if let Some(name) = &args.name {
            validate_sandbox_name(name)?;
            let target = self.resolve_target(name)?;
            if target.exists() {
                return Err(format!(
                    "sandbox destination '{}' already exists",
                    target.display()
                ));
            }
            return Ok((target, false, Some(name.clone())));
        }

        let root = self.sandbox_root();
        if let Err(err) = fs::create_dir_all(&root) {
            return Err(format!(
                "failed to create sandbox root '{}': {}",
                root.display(),
                err
            ));
        }

        let name = self
            .generate_sandbox_name(&args.dependencies, &root)
            .map_err(|err| format!("failed to generate sandbox name: {err}"))?;
        let target = root.join(&name);
        Ok((target, false, Some(name)))
    }

    fn initialize_at(
        &self,
        target: &Path,
        dependencies: &[String],
        allow_existing: bool,
        preferred_id_prefix: Option<&str>,
    ) -> Result<PathBuf, String> {
        if target.exists() {
            if !target.is_dir() {
                return Err(format!(
                    "sandbox destination '{}' exists and is not a directory",
                    target.display()
                ));
            }

            if !allow_existing {
                return Err(format!(
                    "sandbox destination '{}' already exists",
                    target.display()
                ));
            }

            if target.join(".omni").join("id").exists() {
                return Err(format!(
                    "sandbox destination '{}' already contains a work directory",
                    target.display()
                ));
            }

            if target.join(".git").is_dir() {
                return Err(format!(
                    "sandbox destination '{}' already contains a git repository",
                    target.display()
                ));
            }

            if target.join(".omni.yaml").exists() {
                return Err(format!(
                    "sandbox destination '{}' already has an .omni.yaml file",
                    target.display()
                ));
            }
        } else if let Err(err) = fs::create_dir_all(target) {
            return Err(format!(
                "failed to create sandbox directory '{}': {}",
                target.display(),
                err
            ));
        }

        self.write_config(target, dependencies)?;

        let target_str = target
            .to_str()
            .ok_or_else(|| "failed to resolve sandbox path".to_string())?;

        init_workdir(target_str, preferred_id_prefix)
            .map_err(|err| format!("failed to initialize sandbox: {err}"))?;

        Ok(target.to_path_buf())
    }
}

impl BuiltinCommand for SandboxCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["sandbox".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        let sandbox_root = self.sandbox_root();
        let sandbox_root_display = sandbox_root.display();

        Some(format!(
            concat!(
                "Create a sandbox directory pre-configured for omni.\n",
                "\n",
                "The sandbox name is generated automatically unless you pass \x1B[36m--name\x1B[0m.\n",
                "The sandbox is created under {} unless a specific path is provided with \x1B[36m--path\x1B[0m. ",
                "Additional arguments become entries under the `up` section in the generated .omni.yaml.\n",
                "\n",
                "If the target directory already contains a work directory or repository, the command will fail. ",
                "Existing .omni.yaml files are left untouched."
            ),
            sandbox_root_display
        ))
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["-p".to_string(), "--path".to_string()],
                    desc: Some(
                        "Path for the sandbox directory. Relative paths are resolved against the current directory."
                            .to_string(),
                    ),
                    conflicts_with: vec!["name".to_string()],
                    arg_type: SyntaxOptArgType::DirPath,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-n".to_string(), "--name".to_string()],
                    desc: Some("Name for the sandbox directory.".to_string()),
                    conflicts_with: vec!["path".to_string()],
                    arg_type: SyntaxOptArgType::String,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["dependencies".to_string()],
                    desc: Some(
                        "Optional dependencies to add under the `up` section of the generated configuration."
                            .to_string(),
                    ),
                    leftovers: true,
                    allow_hyphen_values: true,
                    required_without: vec!["empty".to_string()],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--allow-empty".to_string()],
                    desc: Some("Create an empty sandbox without any dependencies.".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["Work directories".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        let command = Command::Builtin(self.clone_boxed());
        let args = SandboxCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );
        if let Some(name) = &args.name {
            if let Err(err) = validate_sandbox_name(name) {
                omni_error!(err);
                exit(1);
            }
        }

        let (target_path, allow_existing, preferred_id_prefix) =
            match self.determine_target_path(&args) {
                Ok(value) => value,
                Err(err) => {
                    omni_error!(err);
                    exit(1);
                }
            };

        let init_result = self.initialize_at(
            &target_path,
            &args.dependencies,
            allow_existing,
            preferred_id_prefix.as_deref(),
        );

        let target = match init_result {
            Ok(target) => target,
            Err(err) => {
                omni_error!(err);
                exit(1);
            }
        };

        omni_info!(format!(
            "sandbox initialized at {}",
            target.to_string_lossy().light_blue()
        ));

        if let Err(err) = record_cd(&target) {
            omni_error!(err);
            exit(1);
        }

        let target_str = match target.to_str() {
            Some(path) => path,
            None => {
                omni_error!("failed to resolve sandbox path");
                exit(1);
            }
        };

        if !add_trust(target_str) {
            omni_error!("failed to trust sandbox work directory");
            exit(1);
        }

        if let Err(err) = run_omni_up(&target) {
            omni_error!(err);
            exit(1);
        }

        exit(0);
    }
}

fn record_cd(target: &Path) -> Result<(), String> {
    if omni_cmd_file().is_none() {
        return Ok(());
    }

    let target_str = target
        .to_str()
        .ok_or_else(|| "failed to resolve sandbox path".to_string())?;
    let path_escaped = escape(Cow::Borrowed(target_str));

    omni_cmd_always(format!("cd {path_escaped}").as_str()).map_err(|err| err.to_string())
}

fn run_omni_up(target: &Path) -> Result<(), String> {
    let current_exe_path =
        current_exe().map_err(|err| format!("failed to determine omni executable: {err}"))?;

    omni_info!(format!(
        "running {} in {}",
        "omni up".light_yellow(),
        target.display().to_string().light_cyan()
    ));

    let mut command = std::process::Command::new(current_exe_path);
    command.arg("up");
    command.current_dir(target);
    command.env_remove("OMNI_FORCE_UPDATE");
    command.env("OMNI_SKIP_UPDATE", "1");

    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("omni up failed with status {status}")),
        Err(err) => Err(format!("failed to run omni up: {err}")),
    }
}

#[cfg(test)]
#[path = "sandbox_test.rs"]
mod tests;
