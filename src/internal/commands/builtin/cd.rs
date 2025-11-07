use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::exit;

use shell_escape::escape;

use crate::internal::commands::base::AutocompleteParameter;
use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::base::CommandAutocompletion;
use crate::internal::commands::utils::omni_cmd_on_success;
use crate::internal::commands::utils::path_auto_complete;
use crate::internal::commands::utils::validate_sandbox_name;
use crate::internal::commands::Command;
use crate::internal::config::config;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::omni_cmd_file;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::ORG_LOADER;
use crate::internal::git_env;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;
use crate::omni_warning;

#[derive(Debug, Clone)]
struct CdCommandArgs {
    locate: bool,
    edit: bool,
    include_packages: bool,
    workdir: Option<String>,
}

#[derive(Debug)]
struct WorkdirLocation {
    path: String,
    line_from: Option<u32>,
    line_to: Option<u32>,
}

impl From<BTreeMap<String, ParseArgsValue>> for CdCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let locate = matches!(
            args.get("locate"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );

        let edit = matches!(
            args.get("edit"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );

        let yes_include_packages = matches!(
            args.get("include_packages"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let no_include_packages = matches!(
            args.get("no_include_packages"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let include_packages = if no_include_packages {
            false
        } else if yes_include_packages {
            true
        } else {
            locate
        };

        let workdir = match args.get("workdir") {
            Some(ParseArgsValue::SingleString(Some(workdir))) => Some(workdir.clone()),
            _ => None,
        };

        Self {
            locate,
            edit,
            include_packages,
            workdir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CdCommand {}

impl CdCommand {
    pub fn new() -> Self {
        Self {}
    }

    fn cd_main_org(&self, args: &CdCommandArgs) {
        let path = if let Some(main_org) = ORG_LOADER.first() {
            main_org.worktree()
        } else {
            let config = config(".");
            config.worktree()
        };

        let path_str = path.to_string();

        if args.locate {
            println!("{path_str}");
            exit(0);
        }

        if args.edit {
            self.open_in_editor(&path_str, None, None);
            exit(0);
        }

        let path_escaped = escape(std::borrow::Cow::Borrowed(path_str.as_str()));
        match omni_cmd_on_success(format!("cd {path_escaped}").as_str()) {
            Ok(_) => {}
            Err(e) => {
                omni_error!(e);
                exit(1);
            }
        }
        exit(0);
    }

    fn cd_workdir(&self, wd: &str, args: &CdCommandArgs) {
        if let Some(location) = self.cd_workdir_find(wd, args) {
            if args.locate {
                println!("{}", location.path);
                exit(0);
            }

            if args.edit {
                self.open_in_editor(&location.path, location.line_from, location.line_to);
                exit(0);
            }

            let path_escaped = escape(std::borrow::Cow::Borrowed(location.path.as_str()));
            match omni_cmd_on_success(format!("cd {path_escaped}").as_str()) {
                Ok(_) => {}
                Err(e) => {
                    omni_error!(e);
                    exit(1);
                }
            }
            return;
        }

        if args.locate {
            exit(1);
        }

        omni_error!(format!("{}: No such work directory", wd.yellow()));
        exit(1);
    }

    fn cd_workdir_find(&self, wd: &str, args: &CdCommandArgs) -> Option<WorkdirLocation> {
        // Handle the special case of `...` to go to the work directory root
        if wd == "..." {
            let wd = workdir(".");
            return wd.root().map(|wd_root| WorkdirLocation {
                path: wd_root.to_string(),
                line_from: None,
                line_to: None,
            });
        }

        // Delegate to the shell if this is a path
        if wd.starts_with('/')
            || wd.starts_with('.')
            || wd.starts_with("~/")
            || wd == "~"
            || wd == "-"
        {
            return Some(WorkdirLocation {
                path: wd.to_string(),
                line_from: None,
                line_to: None,
            });
        }

        // Check if the requested wd is actually a path that exists from the current directory
        if let Ok(wd_path) = std::fs::canonicalize(wd) {
            return Some(WorkdirLocation {
                path: format!("{}", wd_path.display()),
                line_from: None,
                line_to: None,
            });
        }

        // Check if this is a git URL (contains :// or starts with http/https)
        if wd.contains("://") || wd.starts_with("http://") || wd.starts_with("https://") {
            return self.handle_git_url(wd, args);
        }

        let only_worktree = !args.include_packages;
        let allow_interactive = !args.locate;

        if let Some(wd_path) = ORG_LOADER.find_repo_quick(wd, only_worktree, false) {
            return Some(WorkdirLocation {
                path: format!("{}", wd_path.display()),
                line_from: None,
                line_to: None,
            });
        }

        if let Some(sandbox_path) = Self::find_sandbox(wd) {
            return Some(WorkdirLocation {
                path: sandbox_path,
                line_from: None,
                line_to: None,
            });
        }

        if let Some(wd_path) =
            ORG_LOADER.find_repo_slow(wd, only_worktree, false, allow_interactive)
        {
            return Some(WorkdirLocation {
                path: format!("{}", wd_path.display()),
                line_from: None,
                line_to: None,
            });
        }

        None
    }

    fn handle_git_url(&self, url: &str, args: &CdCommandArgs) -> Option<WorkdirLocation> {
        use crate::internal::git::utils::safe_git_url_parse;

        // Parse the git URL
        let parsed_url = match safe_git_url_parse(url) {
            Ok(parsed) => parsed,
            Err(_) => return None,
        };

        // Build a search string from the parsed URL (owner/repo or just repo)
        let search_str = if let Some(owner) = &parsed_url.owner {
            format!("{}/{}", owner, parsed_url.name)
        } else {
            parsed_url.name.clone()
        };

        // Find the repository using existing logic
        let only_worktree = !args.include_packages;
        let allow_interactive = !args.locate;

        // Use find_repo which combines quick and slow search
        let repo_path =
            ORG_LOADER.find_repo(&search_str, only_worktree, false, allow_interactive)?;

        // Check if the current ref matches the requested ref
        if let Some(requested_ref) = &parsed_url.git_ref {
            if !Self::check_git_ref(&repo_path, requested_ref, allow_interactive) {
                return None;
            }
        }

        // If we found the repo, potentially append the path from the URL
        let mut final_path = repo_path;
        if let Some(path) = parsed_url.path {
            let full_path = final_path.join(&path);
            final_path = full_path;
        }

        Some(WorkdirLocation {
            path: format!("{}", final_path.display()),
            line_from: parsed_url.line_from,
            line_to: parsed_url.line_to,
        })
    }

    fn find_sandbox(name: &str) -> Option<String> {
        validate_sandbox_name(name).ok()?;

        let sandbox_root = PathBuf::from(config(".").sandbox());
        let candidate = sandbox_root.join(name);
        if !candidate.is_dir() {
            return None;
        }

        match std::fs::canonicalize(&candidate) {
            Ok(path) => Some(path.to_string_lossy().to_string()),
            Err(_) => Some(candidate.to_string_lossy().to_string()),
        }
    }

    fn check_git_ref(repo_path: &PathBuf, requested_ref: &str, allow_interactive: bool) -> bool {
        use git2::Repository;

        let repo = match Repository::open(repo_path) {
            Ok(r) => r,
            Err(_) => return true,
        };

        let head = match repo.head() {
            Ok(h) => h,
            Err(_) => return true,
        };

        let current_ref_name = if head.is_branch() {
            head.shorthand().map(|s| s.to_string())
        } else {
            None
        };

        let current_commit = match head.peel_to_commit() {
            Ok(c) => c,
            Err(_) => return true,
        };

        let repo_id = git_env(repo_path.to_string_lossy().as_ref())
            .id()
            .unwrap_or_else(|| repo_path.display().to_string());

        let requested_commit = match Self::resolve_ref_to_commit(&repo, requested_ref) {
            Some(c) => c,
            None => {
                let message = format!(
                    "could not resolve reference {} in repository {}",
                    requested_ref.yellow(),
                    repo_id.yellow()
                );

                let can_prompt = allow_interactive && shell_is_interactive();
                if !can_prompt {
                    omni_error!(message);
                    exit(1);
                }

                omni_warning!(message);
                if !Self::prompt_proceed() {
                    exit(1);
                }

                return true;
            }
        };

        if current_commit.id() != requested_commit.id() {
            let current_ref_display =
                current_ref_name.unwrap_or_else(|| current_commit.id().to_string());

            let message = format!(
                "repository {} is on {} but URL references {}",
                repo_id.yellow(),
                current_ref_display.yellow(),
                requested_ref.yellow()
            );

            let can_prompt = allow_interactive && shell_is_interactive();
            if !can_prompt {
                omni_error!(message);
                exit(1);
            }

            omni_warning!(message);
            if !Self::prompt_proceed() {
                exit(1);
            }
        }

        true
    }

    fn prompt_proceed() -> bool {
        let question = requestty::Question::confirm("proceed_with_ref_mismatch")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message("Continue?")
            .default(true)
            .build();

        match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::Bool(confirmed) => confirmed,
                _ => false,
            },
            Err(_) => false,
        }
    }

    fn resolve_ref_to_commit<'a>(
        repo: &'a git2::Repository,
        ref_name: &str,
    ) -> Option<git2::Commit<'a>> {
        if let Ok(obj) = repo.revparse_single(ref_name) {
            if let Ok(commit) = obj.peel_to_commit() {
                return Some(commit);
            }
        }

        if let Ok(oid) = git2::Oid::from_str(ref_name) {
            if let Ok(commit) = repo.find_commit(oid) {
                return Some(commit);
            }
        }

        None
    }

    fn find_editor() -> Option<String> {
        // Try VISUAL first
        if let Ok(visual) = std::env::var("VISUAL") {
            if !visual.is_empty() {
                return Some(visual);
            }
        }

        // Then try EDITOR
        if let Ok(editor) = std::env::var("EDITOR") {
            if !editor.is_empty() {
                return Some(editor);
            }
        }

        // Try vim
        if let Ok(vim) = which::which("vim") {
            return Some(vim.to_string_lossy().to_string());
        }

        // Try nano
        if let Ok(nano) = which::which("nano") {
            return Some(nano.to_string_lossy().to_string());
        }

        None
    }

    fn open_in_editor(&self, path: &str, line_from: Option<u32>, line_to: Option<u32>) {
        let editor = match Self::find_editor() {
            Some(e) => e,
            None => {
                omni_error!(
                    "no editor found - please set the VISUAL or EDITOR environment variables."
                );
                exit(1);
            }
        };

        // Build the command with line number if provided
        let mut cmd = std::process::Command::new(&editor);

        // Editor binary name
        let editor_path = PathBuf::from(&editor);
        let bin_name = editor_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&editor);

        // Handle line number syntax for common editors
        // Note: line_to is currently only used for vim/nvim visual selection
        match bin_name {
            "code" | "code-insiders" | "cursor" => {
                // VSCode uses --goto for line numbers
                // No native CLI support for line ranges, so we just go to line_from
                if let Some(line) = line_from {
                    cmd.arg("--goto");
                    cmd.arg(format!("{}:{}", path, line));
                } else {
                    cmd.arg(path);
                }
            }
            "subl" | "sublime_text" => {
                // Sublime Text uses :line:col syntax
                // No native CLI support for line ranges, so we just go to line_from
                if let Some(line) = line_from {
                    cmd.arg(format!("{}:{}", path, line));
                } else {
                    cmd.arg(path);
                }
            }
            "vim" | "nvim" => {
                // Vim/Neovim: use visual line selection if we have a range
                if let (Some(from), Some(to)) = (line_from, line_to) {
                    if from != to {
                        // Select from line_from to line_to using visual line mode
                        let lines_to_select = to.saturating_sub(from);
                        cmd.arg(format!(
                            "+call cursor({}, 1) | normal! V{}j",
                            from, lines_to_select
                        ));
                    } else {
                        cmd.arg(format!("+{}", from));
                    }
                } else if let Some(line) = line_from {
                    cmd.arg(format!("+{}", line));
                }
                cmd.arg(path);
            }
            "nano" | "emacs" => {
                // These editors use +line syntax but don't have easy CLI range selection
                if let Some(line) = line_from {
                    cmd.arg(format!("+{}", line));
                }
                cmd.arg(path);
            }
            _ => {
                // Generic case: just pass the path
                cmd.arg(path);
            }
        }

        // Replace the current process with the editor
        let err = cmd.exec();

        // exec() only returns if there's an error
        omni_error!(format!("Failed to execute editor '{}': {}", editor, err));
        exit(1);
    }
}

impl BuiltinCommand for CdCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["cd".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Change directory to the root of the specified work directory\n",
                "\n",
                "If no work directory is specified, change to the git directory of the main org as ",
                "specified by \x1B[3mOMNI_ORG\x1B[0m, if specified, or errors out if not ",
                "specified.\n",
                "\n",
                "This command also supports a number of git URL formats. When a URL is provided, the command ",
                "will find the corresponding local repository and navigate to the specified path if ",
                "included in the URL.",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["-l".to_string(), "--locate".to_string()],
                    desc: Some(
                        concat!(
                            "If provided, will only return the path to the work directory instead of switching ",
                            "directory to it. When this flag is passed, interactions are also disabled, ",
                            "as it is assumed to be used for command line purposes. ",
                            "This will exit with 0 if the work directory is found, 1 otherwise.",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    conflicts_with: vec!["--edit".to_string()],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-e".to_string(), "--edit".to_string()],
                    desc: Some(
                        concat!(
                            "If provided, will open the work directory in the editor specified by ",
                            "\x1B[3mVISUAL\x1B[0m or \x1B[3mEDITOR\x1B[0m environment variables, ",
                            "or fallback to vim or nano if available. When this flag is passed, ",
                            "interactions are also disabled.",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    conflicts_with: vec!["--locate".to_string()],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-p".to_string(), "--include-packages".to_string()],
                    desc: Some(
                        concat!(
                            "If provided, will include packages when running the command; ",
                            "this defaults to including packages when using \x1B[3m--locate\x1B[0m, ",
                            "and not including packages otherwise.",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    conflicts_with: vec!["--no-include-packages".to_string()],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--no-include-packages".to_string()],
                    desc: Some(
                        concat!(
                            "If provided, will NOT include packages when running the command; ",
                            "this defaults to including packages when using \x1B[3m--locate\x1B[0m, ",
                            "and not including packages otherwise.",
                        )
                        .to_string()
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["workdir".to_string()],
                    desc: Some(
                        concat!(
                            "The name of the work directory to change directory to; this can be in the format ",
                            "<org>/<repo>, or just <repo>, in which case the work directory will be searched for ",
                            "in all the organizations, trying to use \x1B[3mOMNI_ORG\x1B[0m if it is set, and then ",
                            "trying all the other organizations alphabetically.",
                        )
                        .to_string()
                    ),
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["Git commands".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        let command = Command::Builtin(self.clone_boxed());
        let args = CdCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        if omni_cmd_file().is_none() && !args.locate && !args.edit {
            omni_error!("not available without the shell integration");
            exit(1);
        }

        if let Some(workdir) = &args.workdir {
            self.cd_workdir(workdir, &args);
        } else {
            self.cd_main_org(&args);
        }
        exit(0);
    }

    fn autocompletion(&self) -> CommandAutocompletion {
        CommandAutocompletion::Partial
    }

    fn autocomplete(
        &self,
        comp_cword: usize,
        argv: Vec<String>,
        parameter: Option<AutocompleteParameter>,
    ) -> Result<(), ()> {
        // We only have the work directory to autocomplete
        if let Some(param) = parameter {
            if param.name == "workdir" {
                let repo = argv.get(comp_cword).map_or("", String::as_str);

                path_auto_complete(repo, true, false)
                    .iter()
                    .for_each(|s| println!("{s}"));
            }
        }

        Ok(())
    }
}
