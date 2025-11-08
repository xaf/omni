use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use normalize_path::NormalizePath;
use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::global_config;
use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::up::github_release::UpConfigGithubRelease;
use crate::internal::config::up::mise::FullyQualifiedToolName;
use crate::internal::config::up::mise::PostInstallFuncArgs;
use crate::internal::config::up::mise_tool_path;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::MiseToolUpVersion;
use crate::internal::config::up::UpConfigMise;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::utils::is_executable;
use crate::internal::dynenv::update_dynamic_env_for_command_from_env;
use crate::internal::env::current_dir;
use crate::internal::env::tmpdir_cleanup_prefix;
use crate::internal::env::workdir;
use crate::internal::user_interface::StringColor;
use crate::internal::ConfigValue;

const MIN_VERSION_VENV: Version = Version::new(3, 3, 0);
// const MIN_VERSION_VIRTUALENV: Version = Version::new(2, 6, 0);

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpConfigPythonParams {
    #[serde(default, rename = "pip", skip_serializing_if = "Vec::is_empty")]
    pip_files: Vec<String>,
    #[serde(default, skip)]
    pip_auto: bool,
    #[serde(default, skip)]
    pip_disabled: bool,
}

impl UpConfigPythonParams {
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let mut pip_files = Vec::new();
        let mut pip_auto = false;
        let mut pip_disabled = false;

        if let Some(config_value) = config_value {
            if let Some(pip) = config_value.get("pip") {
                let error_handler = error_handler.with_key("pip");
                if let Some(pip_array) = pip.as_array() {
                    for (idx, file_path) in pip_array.iter().enumerate() {
                        if let Some(file_path) = file_path.as_str_forced() {
                            pip_files.push(file_path.to_string());
                        } else {
                            error_handler
                                .with_index(idx)
                                .with_expected("string")
                                .with_actual(file_path)
                                .error(ConfigErrorKind::InvalidValueType);
                        }
                    }
                } else if let Some(value) = pip.as_bool_forced() {
                    if value {
                        pip_auto = true;
                    } else {
                        pip_disabled = true;
                    }
                } else if let Some(file_path) = pip.as_str_forced() {
                    match file_path.as_str() {
                        "auto" => pip_auto = true,
                        _ => pip_files.push(file_path),
                    }
                } else {
                    error_handler
                        .with_expected(vec!["string", "sequence", "boolean"])
                        .with_actual(pip)
                        .error(ConfigErrorKind::InvalidValueType);
                }
            }
        }

        Self {
            pip_files,
            pip_auto,
            pip_disabled,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfigPython {
    #[serde(skip)]
    pub backend: UpConfigMise,
    #[serde(skip)]
    pub params: UpConfigPythonParams,
}

impl Serialize for UpConfigPython {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        // Serialize object into serde_json::Value
        let mut backend = serde_json::to_value(&self.backend).unwrap();

        // Serialize the params object
        let mut params = serde_json::to_value(&self.params).unwrap();

        // If params.pip_auto is true, set the pip field to "auto"
        if self.params.pip_auto {
            params["pip"] = serde_json::Value::String("auto".to_string());
        }

        // Merge the params object into the base object
        backend
            .as_object_mut()
            .unwrap()
            .extend(params.as_object().unwrap().clone());

        // Serialize the object
        backend.serialize(serializer)
    }
}

impl UpConfigPython {
    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let mut backend = UpConfigMise::from_config_value("python", config_value, error_handler);
        backend.add_detect_version_func(detect_version_from_pyproject_toml);
        backend.add_post_install_func(setup_python_venv);
        backend.add_post_install_func(setup_python_requirements);

        let params = UpConfigPythonParams::from_config_value(config_value, error_handler);

        Self { backend, params }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        self.backend.up(options, environment, progress_handler)
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        self.backend.down(progress_handler)
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        // Get the data paths from the backend
        let mut data_paths = self.backend.data_paths();

        // Add the tools directory
        if let Some(data_path) = workdir(".").data_path() {
            data_paths.push(data_path.join("python").join(".tools"));
        }

        data_paths
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UvBin {
    bin: PathBuf,
}

/// The path to the uv binary
static UV_BIN: OnceLock<UvBin> = OnceLock::new();

impl UvBin {
    fn get(options: &UpOptions, progress_handler: &UpProgressHandler) -> Result<Self, UpError> {
        // First, check if we already have found and cached the uv binary path
        if let Some(uv_bin) = UV_BIN.get() {
            return Ok(uv_bin.clone());
        }

        // If we get here, let's make sure we get the uv binary from GitHub
        let gh_release = UpConfigGithubRelease::new_with_version(
            "astral-sh/uv",
            &global_config().up_command.uv_version,
            false, // We do not force the upgrade here, since we want to use any
                   // version of uv that satisfies the version constraint
        );

        // We create a fake environment since we do not want to add this
        // release to the environment, we just want the uv binary
        let mut fake_env = UpEnvironment::new();

        let subhandler = progress_handler.subhandler(&"uv: ".light_black());
        gh_release.up(options, &mut fake_env, &subhandler)?;

        // Check that the uv binary is installed
        let install_path = gh_release.install_path()?;
        let install_bin_path = install_path.join("bin");
        let install_bin = if install_bin_path.is_dir() {
            install_bin_path.join("uv")
        } else {
            install_path.join("uv")
        };

        if !install_bin.exists() || !is_executable(&install_bin) {
            return Err(UpError::Exec(
                "failed to install uv: binary not found".to_string(),
            ));
        }

        // Create the uv binary instance
        let uv_bin = Self { bin: install_bin };

        // Cache the uv binary path
        let _ = UV_BIN.set(uv_bin.clone()); // We ignore the error here, since we know the lock is not set yet

        Ok(uv_bin)
    }
}

/// Extracts Python version requirements from a pyproject.toml file
///
/// This function attempts to read Python version constraints from various standard
/// locations in a pyproject.toml file, including:
/// - project.requires-python
/// - build-system.requires (looking for Python version specs)
/// - tool.poetry.dependencies.python
///
/// Returns a string containing the Python version constraint or None if not found
fn detect_version_from_pyproject_toml(_tool_name: String, path: PathBuf) -> Option<String> {
    // Check for pyproject.toml file
    let pyproject_path = path.join("pyproject.toml");
    if !pyproject_path.exists() || pyproject_path.is_dir() {
        return None;
    }

    // Read the file content
    let pyproject_str = match std::fs::read_to_string(&pyproject_path) {
        Ok(content) => content,
        Err(_) => return None,
    };

    // Parse the TOML content
    let pyproject: toml::Value = match toml::from_str(&pyproject_str) {
        Ok(parsed) => parsed,
        Err(_) => return None,
    };

    // Check for project.requires-python (PEP 621 standard)
    if let Some(project) = pyproject.get("project").and_then(|p| p.as_table()) {
        if let Some(requires_python) = project.get("requires-python").and_then(|v| v.as_str()) {
            return Some(requires_python.to_string());
        }
    }

    // Check for tool.poetry.dependencies.python (Poetry)
    if let Some(tool) = pyproject.get("tool").and_then(|t| t.as_table()) {
        if let Some(poetry) = tool.get("poetry").and_then(|p| p.as_table()) {
            if let Some(deps) = poetry.get("dependencies").and_then(|d| d.as_table()) {
                if let Some(python_ver) = deps.get("python") {
                    let version = match python_ver {
                        toml::Value::String(v) => Some(v.clone()),
                        toml::Value::Table(t) => {
                            t.get("version").and_then(|v| v.as_str()).map(String::from)
                        }
                        _ => None,
                    };

                    if let Some(version) = version {
                        return Some(version);
                    }
                }
            }
        }
    }

    // Check for build-system.requires (PEP 518)
    // This is less reliable as it contains build dependencies, but we can look for
    // Python version constraints in the list
    if let Some(build_system) = pyproject.get("build-system").and_then(|b| b.as_table()) {
        if let Some(requires) = build_system.get("requires").and_then(|r| r.as_array()) {
            for req in requires {
                if let Some(req_str) = req.as_str() {
                    // Look for requirements like "python>=3.7" or "python_version>='3.8'"
                    if req_str.starts_with("python") && req_str.contains(['>', '<', '=']) {
                        // Extract just the version constraint part
                        let version = req_str
                            .split_once(['>', '<', '='])
                            .map(|(_, v)| v.trim().replace(['\'', '"'], "").to_string());

                        return version;
                    }
                }
            }
        }
    }

    None
}

fn setup_python_venv(
    options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &UpProgressHandler,
    args: &PostInstallFuncArgs,
) -> Result<(), UpError> {
    if args.fqtn.tool() != "python" {
        panic!(
            "setup_python_venv called with wrong tool: {}",
            args.fqtn.tool()
        );
    }

    // Handle each version individually
    for version in &args.versions {
        setup_python_venv_per_version(
            options,
            environment,
            progress_handler,
            args.fqtn,
            version.clone(),
        )?;
    }

    Ok(())
}

fn setup_python_venv_per_version(
    options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &UpProgressHandler,
    fqtn: &FullyQualifiedToolName,
    version: MiseToolUpVersion,
) -> Result<(), UpError> {
    // Check if we care about that version
    match Version::parse(&version.version) {
        Ok(version) => {
            if version < MIN_VERSION_VENV {
                progress_handler.progress(format!(
                    "skipping venv setup for python {version} < {MIN_VERSION_VENV}"
                ));
                return Ok(());
            }
        }
        Err(_) => {
            progress_handler.progress(format!(
                "skipping venv setup for python {} (unsupported version)",
                version.version
            ));
            return Ok(());
        }
    }

    for dir in version.dirs {
        setup_python_venv_per_dir(
            options,
            environment,
            progress_handler,
            fqtn,
            version.version.clone(),
            dir,
        )?;
    }

    Ok(())
}

fn setup_python_venv_per_dir(
    options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &UpProgressHandler,
    fqtn: &FullyQualifiedToolName,
    version: String,
    dir: String,
) -> Result<(), UpError> {
    // Get the data path for the work directory
    let workdir = workdir(".");

    let data_path = if let Some(data_path) = workdir.data_path() {
        data_path
    } else {
        return Err(UpError::Exec(format!(
            "failed to get data path for {}",
            current_dir().display()
        )));
    };

    // Get the hash of the relative path
    let venv_dir = data_path_dir_hash(&dir);

    let venv_path = data_path
        .join(fqtn.normalized_plugin_name()?)
        .join(version.clone())
        .join(venv_dir.clone());

    // Check if we need to install, or if the virtual env is already there
    let already_setup = if venv_path.exists() {
        if venv_path.join("pyvenv.cfg").exists() {
            progress_handler.progress(format!("venv already exists for python {version}"));
            true
        } else {
            // Remove the directory since it exists but is not a venv,
            // so we clean it up and replace it by a clean venv
            std::fs::remove_dir_all(&venv_path).map_err(|_| {
                UpError::Exec(format!(
                    "failed to remove existing venv directory {}",
                    venv_path.display()
                ))
            })?;
            false
        }
    } else {
        false
    };

    let normalized_name = fqtn.normalized_plugin_name()?;

    // Only create the new venv if it doesn't exist
    if !already_setup {
        let python_version_path = mise_tool_path(&normalized_name, &version);
        let python_bin = PathBuf::from(python_version_path)
            .join("bin")
            .join("python");

        let uv_bin = UvBin::get(options, progress_handler)?;

        let mut venv_create = TokioCommand::new(&uv_bin.bin);
        venv_create.arg("venv");
        venv_create.arg("--seed");
        venv_create.arg("--python");
        venv_create.arg(python_bin);
        venv_create.arg("--no-python-downloads");
        venv_create.arg(venv_path.to_string_lossy().to_string());
        venv_create.stdout(std::process::Stdio::piped());
        venv_create.stderr(std::process::Stdio::piped());

        run_progress(
            &mut venv_create,
            Some(progress_handler),
            RunConfig::default(),
        )?;

        progress_handler.progress(format!(
            "venv created for python {} in {}",
            version,
            if dir.is_empty() { "." } else { &dir }
        ));
    }

    // Update the cache
    environment.add_version_data_path(
        &normalized_name,
        &version,
        &dir,
        &venv_path.to_string_lossy(),
    );

    Ok(())
}

fn setup_python_requirements(
    options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &UpProgressHandler,
    args: &PostInstallFuncArgs,
) -> Result<(), UpError> {
    let params = UpConfigPythonParams::from_config_value(
        args.config_value.as_ref(),
        &ConfigErrorHandler::noop(),
    );

    // If pip is disabled, skip dependency installation
    if params.pip_disabled {
        return Ok(());
    }

    // Try and detect dependencies automatically if either requested or if there
    // are no dependencies specified
    let pip_auto = params.pip_auto || params.pip_files.is_empty();

    let tool_dirs = args
        .versions
        .iter()
        .flat_map(|version| version.dirs.clone())
        .collect::<Vec<String>>();

    for dir in &tool_dirs {
        let path = PathBuf::from(dir).normalize();

        // Check if path is in current dir
        let full_path = abs_path(dir);
        if !full_path.starts_with(current_dir()) {
            return Err(UpError::Exec(format!(
                "directory {} is not in work directory",
                path.display(),
            )));
        }

        // Load the environment for that directory
        update_dynamic_env_for_command_from_env(full_path.to_string_lossy(), environment);

        // Determine which files to check based on pip_auto
        let dependency_files: Vec<PathBuf> = if pip_auto {
            let first_file = [
                "poetry.lock",
                "Pipfile.lock",
                "pyproject.toml",
                "requirements.txt",
                "Pipfile",
            ]
            .iter()
            .map(|f| full_path.join(f))
            .find(|f| f.exists())
            .map(|pb| pb.to_path_buf()); // Use map instead of cloned for Option<PathBuf>

            if let Some(file) = first_file {
                vec![file]
            } else {
                vec![]
            }
        } else {
            // Use the specified files directly
            params.pip_files.iter().map(|f| full_path.join(f)).collect()
        };

        // Install dependencies from all the found files
        for dep_file in dependency_files {
            setup_python_requirements_file(options, progress_handler, dep_file)?;
        }
    }

    Ok(())
}

struct IsolatedPythonTool {
    bin_path: PathBuf,
    python_path: PathBuf,
}

impl IsolatedPythonTool {
    fn new(
        tool_name: &str,
        extra_packages: &[&str],
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<Self, UpError> {
        // If the tool is already installed, we'll find a binary
        // in <data_path>/.tools/bin/<tool_name>, and we can return immediately
        let wd = workdir(".");
        let data_path = wd.data_path().ok_or_else(|| {
            UpError::Exec("failed to get data path for current directory".to_string())
        })?;
        let tools_path = data_path.join("python").join(".tools");
        let bin_path = tools_path.join("bin").join(tool_name);

        let object = Self {
            bin_path: bin_path.clone(),
            python_path: tools_path.clone(),
        };

        if !bin_path.exists() {
            // If the tool is not installed, we need to install it
            let uv_bin = UvBin::get(options, progress_handler)?;

            // Install the tool using uv
            let mut uv_install = TokioCommand::new(&uv_bin.bin);
            uv_install.arg("pip");
            uv_install.arg("install");
            uv_install.arg("--target");
            uv_install.arg(tools_path.to_string_lossy().to_string());
            uv_install.arg(tool_name);
            for package in extra_packages {
                uv_install.arg(package);
            }
            uv_install.stdout(std::process::Stdio::piped());
            uv_install.stderr(std::process::Stdio::piped());

            run_progress(
                &mut uv_install,
                Some(progress_handler),
                RunConfig::default(),
            )?;
        }

        if !bin_path.exists() {
            return Err(UpError::Exec(format!(
                "failed to install {tool_name}: binary not found after installation"
            )));
        }

        Ok(object)
    }

    // Return the PYTHONPATH prefixed with the tool's python path
    fn get_python_path(&self) -> String {
        let current = std::env::var("PYTHONPATH").unwrap_or_default();
        let python_path = self.python_path.to_string_lossy().to_string();
        if current.is_empty() {
            python_path
        } else {
            format!("{python_path}:{current}")
        }
    }

    fn get_tokio_command(&self) -> TokioCommand {
        let mut cmd = TokioCommand::new(&self.bin_path);
        cmd.env("PYTHONPATH", self.get_python_path());
        cmd
    }
}

/// Create a temporary file for exporting requirements
fn create_temp_requirements_file(prefix: &str) -> Result<tempfile::NamedTempFile, UpError> {
    tempfile::Builder::new()
        .prefix(&tmpdir_cleanup_prefix(&format!("omni_up_python_{prefix}")))
        .suffix(".txt")
        .tempfile()
        .map_err(|e| UpError::Exec(format!("failed to create temporary file: {e}")))
}

fn setup_python_requirements_file(
    options: &UpOptions,
    progress_handler: &UpProgressHandler,
    requirements_file: PathBuf,
) -> Result<(), UpError> {
    if !requirements_file.exists() {
        return Err(UpError::Exec(format!(
            "file {} does not exist",
            requirements_file.display()
        )));
    }

    let relative_path = requirements_file
        .strip_prefix(current_dir())
        .unwrap_or(&requirements_file)
        .display();
    progress_handler.progress(format!(
        "installing dependencies from {}",
        relative_path.light_yellow()
    ));

    // Get uv binary
    let uv_bin = UvBin::get(options, progress_handler)?;

    // Determine file type based on file name
    let file_name = requirements_file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    // Process the file based on type and get the final requirements path to use
    // The _tmp_file is kept in scope to prevent early deletion
    let (requirements_path, _tmp_file) = match file_name.as_str() {
        "poetry.lock" => {
            extract_poetry_lock_requirements(options, &requirements_file, progress_handler)?
        }
        "Pipfile.lock" | "Pipfile" => {
            extract_pipfile_requirements(options, &requirements_file, progress_handler)?
        }
        f if f.ends_with(".lock") => {
            return Err(UpError::Exec(format!(
                "unsupported lock file format: {relative_path}",
            )))
        }
        _ => (requirements_file.clone(), None),
    };

    let mut uv_install = TokioCommand::new(&uv_bin.bin);
    uv_install.arg("pip");
    uv_install.arg("install");
    uv_install.arg("-r");
    uv_install.arg(requirements_path.to_string_lossy().to_string());
    uv_install.stdout(std::process::Stdio::piped());
    uv_install.stderr(std::process::Stdio::piped());

    run_progress(
        &mut uv_install,
        Some(progress_handler),
        RunConfig::default(),
    )?;

    progress_handler.progress(format!(
        "dependencies from {} installed",
        relative_path.light_yellow()
    ));

    Ok(())
}

fn extract_poetry_lock_requirements(
    options: &UpOptions,
    _requirements_file: &Path,
    progress_handler: &UpProgressHandler,
) -> Result<(PathBuf, Option<tempfile::NamedTempFile>), UpError> {
    // Check if poetry is available or install it
    let poetry = IsolatedPythonTool::new(
        "poetry",
        &["poetry-plugin-export"],
        options,
        progress_handler,
    )?;

    // Create a temporary file for the requirements
    let tmp_file = create_temp_requirements_file("poetry")?;
    let tmp_path = tmp_file.path();

    // Run poetry export to generate requirements file
    progress_handler.progress("exporting poetry.lock to requirements file".to_string());

    let mut poetry_export = poetry.get_tokio_command();
    poetry_export.arg("export");
    poetry_export.arg("--without-hashes");
    poetry_export.arg("--format=requirements.txt");
    poetry_export.arg("--output");
    poetry_export.arg(tmp_path.to_string_lossy().to_string());
    poetry_export.stdout(std::process::Stdio::piped());
    poetry_export.stderr(std::process::Stdio::piped());

    run_progress(
        &mut poetry_export,
        Some(progress_handler),
        RunConfig::default(),
    )?;

    Ok((tmp_path.to_path_buf(), Some(tmp_file)))
}

/// Extract requirements from a Pipfile or Pipfile.lock using pipenv
fn extract_pipfile_requirements(
    options: &UpOptions,
    requirements_file: &Path,
    progress_handler: &UpProgressHandler,
) -> Result<(PathBuf, Option<tempfile::NamedTempFile>), UpError> {
    // Check if pipenv is available or install it
    let pipenv = IsolatedPythonTool::new("pipenv", &[], options, progress_handler)?;

    // Get the file name to determine the file type
    let file_name = requirements_file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    // Create a temporary file for the requirements with an appropriate prefix
    let tmp_file = create_temp_requirements_file("pipenv")?;
    let tmp_path = tmp_file.path();

    // Get the directory containing the Pipfile/Pipfile.lock
    let pipfile_dir = requirements_file.parent().ok_or_else(|| {
        UpError::Exec(format!(
            "failed to get parent directory for {}",
            requirements_file.display()
        ))
    })?;

    // Run pipenv requirements to generate requirements file
    progress_handler.progress(format!("exporting {file_name} to requirements file"));

    let mut pipenv_export = pipenv.get_tokio_command();
    // Set working directory to where the Pipfile/Pipfile.lock is located
    pipenv_export.current_dir(pipfile_dir);
    pipenv_export.arg("requirements");
    pipenv_export.env("PIPENV_VERBOSITY", "-1");

    // Open the file for writing - create a new file or truncate existing file
    let stdout_file = std::fs::File::create(tmp_path)
        .map_err(|e| UpError::Exec(format!("failed to create output file: {e}")))?;

    pipenv_export.stdout(stdout_file);
    pipenv_export.stderr(std::process::Stdio::piped());

    run_progress(
        &mut pipenv_export,
        Some(progress_handler),
        RunConfig::default(),
    )?;

    Ok((tmp_path.to_path_buf(), Some(tmp_file)))
}

#[cfg(test)]
#[path = "python_test.rs"]
mod tests;
