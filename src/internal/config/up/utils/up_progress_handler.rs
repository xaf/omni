use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::io::BufRead;
use std::io::Write;
use std::process::exit;
use std::time::Duration as StdDuration;
use std::time::Instant as StdInstant;

use fs4::fs_std::FileExt as Fs4FileExt;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::global_config;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::shell_is_interactive;
use crate::internal::errors::SyncUpdateError;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;
use crate::omni_info;
use crate::omni_warning;

#[cfg(test)]
use crate::internal::config::up::utils::VoidProgressHandler;

pub struct UpProgressHandler<'a> {
    handler: OnceCell<Box<dyn ProgressHandler>>,
    handler_id: Option<String>,
    step: Option<(usize, usize)>,
    prefix: String,
    parent: Option<&'a UpProgressHandler<'a>>,
    allow_ending: bool,
    sync_file: Option<&'a std::fs::File>,
    desc: OnceCell<String>,
}

impl Default for UpProgressHandler<'_> {
    fn default() -> Self {
        UpProgressHandler {
            handler: OnceCell::new(),
            handler_id: None,
            step: None,
            prefix: "".to_string(),
            parent: None,
            allow_ending: true,
            sync_file: None,
            desc: OnceCell::new(),
        }
    }
}

impl<'a> UpProgressHandler<'a> {
    #[cfg(test)]
    pub fn new_void() -> Self {
        let handler = VoidProgressHandler::new();

        let new = UpProgressHandler {
            handler: OnceCell::new(),
            ..Default::default()
        };

        if new.handler.set(Box::new(handler)).is_err() {
            panic!("failed to set progress handler");
        }

        new
    }

    pub fn new(progress: Option<(usize, usize)>) -> Self {
        // Generate a random handler ID
        let handler_id = uuid::Uuid::new_v4().to_string();

        UpProgressHandler {
            handler_id: Some(handler_id),
            step: progress,
            ..Default::default()
        }
    }

    pub fn desc(&self) -> &str {
        if let Some(parent) = self.parent {
            return parent.desc();
        }

        self.desc.get_or_init(|| "".to_string()).as_str()
    }

    pub fn init(&self, desc: String) -> bool {
        if self.handler.get().is_some() || self.parent.is_some() {
            return false;
        }

        if self.desc.set(desc.clone()).is_err() {
            panic!("failed to set progress description");
        }

        #[cfg(not(test))]
        let boxed_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
            Box::new(SpinnerProgressHandler::new(desc, self.step))
        } else {
            Box::new(PrintProgressHandler::new(desc, self.step))
        };

        #[cfg(test)]
        let boxed_handler: Box<dyn ProgressHandler> =
            Box::new(PrintProgressHandler::new(desc, self.step));

        if self.handler.set(boxed_handler).is_err() {
            panic!("failed to set progress handler");
        }
        true
    }

    fn handler(&self) -> &dyn ProgressHandler {
        if let Some(parent) = self.parent {
            return parent.handler();
        }

        self.handler
            .get_or_init(|| {
                let desc = "".to_string();
                let boxed_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
                    Box::new(SpinnerProgressHandler::new(desc, self.step))
                } else {
                    Box::new(PrintProgressHandler::new(desc, self.step))
                };
                boxed_handler
            })
            .as_ref()
    }

    fn handler_id(&self) -> String {
        if let Some(handler_id) = &self.handler_id {
            return handler_id.clone();
        }

        if let Some(parent) = self.parent {
            return parent.handler_id();
        }

        "".to_string()
    }

    pub fn subhandler(&'a self, prefix: &dyn ToString) -> UpProgressHandler<'a> {
        UpProgressHandler {
            handler: OnceCell::new(),
            handler_id: None,
            step: None,
            prefix: prefix.to_string(),
            parent: Some(self),
            allow_ending: false,
            sync_file: None,
            desc: OnceCell::new(),
        }
    }

    pub fn step(&self) -> Option<(usize, usize)> {
        if let Some(parent) = self.parent {
            parent.step()
        } else {
            self.step
        }
    }

    pub fn set_sync_file(&mut self, sync_file: &'a std::fs::File) {
        self.sync_file = Some(sync_file);
    }

    fn update_sync_file(&self, action: SyncUpdateProgressAction) {
        if let Some(sync_file) = self.sync_file {
            // Overwrite the handler id and description with the current ones
            let update = SyncUpdateOperation::Progress(SyncUpdateProgress {
                handler_id: self.handler_id(),
                desc: self.desc().to_string(),
                step: self.step(),
                action,
            });

            if let Err(err) = update.dump_to_file(sync_file) {
                panic!("failed to write progress update to file: {err}");
            }
        } else if let Some(parent) = self.parent {
            parent.update_sync_file(action);
        }
    }

    pub fn perform_sync_action(&self, action: &SyncUpdateProgressAction) {
        match action {
            SyncUpdateProgressAction::Progress(message) => self.handler().progress(message.clone()),
            SyncUpdateProgressAction::Success(message) => {
                if let Some(message) = message {
                    self.handler().success_with_message(message.clone());
                } else {
                    self.handler().success();
                }
            }
            SyncUpdateProgressAction::Error(message) => {
                if let Some(message) = message {
                    self.handler().error_with_message(message.clone());
                } else {
                    self.handler().error();
                }
            }
            SyncUpdateProgressAction::Hide => self.handler().hide(),
            SyncUpdateProgressAction::Show => self.handler().show(),
            SyncUpdateProgressAction::Println(message) => self.handler().println(message.clone()),
        }
    }

    fn format_message(&self, message: String) -> String {
        let message = format!("{}{}", self.prefix, message);
        match self.parent {
            Some(parent) => parent.format_message(message),
            None => message,
        }
    }
}

impl ProgressHandler for UpProgressHandler<'_> {
    fn progress(&self, message: String) {
        let message = self.format_message(message);
        self.update_sync_file(SyncUpdateProgressAction::Progress(message.clone()));
        self.handler().progress(message);
    }

    fn success(&self) {
        self.update_sync_file(SyncUpdateProgressAction::Success(None));
        self.handler().success();
    }

    fn success_with_message(&self, message: String) {
        let message = self.format_message(message);
        if self.allow_ending {
            self.update_sync_file(SyncUpdateProgressAction::Success(Some(message.clone())));
            self.handler().success_with_message(message);
        } else {
            self.update_sync_file(SyncUpdateProgressAction::Progress(message.clone()));
            self.handler().progress(message);
        }
    }

    fn error(&self) {
        if self.allow_ending {
            self.update_sync_file(SyncUpdateProgressAction::Error(None));
            self.handler().error();
        }
    }

    fn error_with_message(&self, message: String) {
        let message = self.format_message(message);
        if self.allow_ending {
            self.update_sync_file(SyncUpdateProgressAction::Error(Some(message.clone())));
            self.handler().error_with_message(message);
        } else {
            self.update_sync_file(SyncUpdateProgressAction::Progress(message.clone()));
            self.handler().progress(message);
        }
    }

    fn hide(&self) {
        self.update_sync_file(SyncUpdateProgressAction::Hide);
        self.handler().hide();
    }

    fn show(&self) {
        self.update_sync_file(SyncUpdateProgressAction::Show);
        self.handler().show();
    }

    fn println(&self, message: String) {
        let message = self.format_message(message);
        self.update_sync_file(SyncUpdateProgressAction::Println(message.clone()));
        self.handler().println(message);
    }
}

pub struct SyncUpdateListener<'a> {
    expected_init: Option<SyncUpdateInit>,
    current_handler: Option<UpProgressHandler<'a>>,
    current_handler_id: Option<String>,
    seen_init: bool,
    missing_options: bool,
    attached_pid: Option<u32>,
    since_modified: u64,
}

impl SyncUpdateListener<'_> {
    pub fn new() -> Self {
        Self {
            expected_init: None,
            current_handler: None,
            current_handler_id: None,
            seen_init: false,
            missing_options: false,
            attached_pid: None,
            since_modified: 0,
        }
    }

    pub fn expect_init(&mut self, init: &SyncUpdateInit) -> &mut Self {
        self.expected_init = Some(init.clone());
        self
    }

    pub fn follow(&mut self, file: &std::fs::File) -> Result<bool, SyncUpdateError> {
        let mut lines = std::io::BufReader::new(file).lines();

        self.since_modified = file
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map_or(0, |d| d.as_secs());

        self.current_handler = None;
        self.current_handler_id = None;

        // The timeout duration after which we suggest to kill the process
        let timeout_duration =
            StdDuration::from_secs(global_config().up_command.attach_kill_timeout);

        // The last time we saw activity
        let mut last_activity = StdInstant::now();

        // Marker that indicates if we are in the first loop
        let mut first_loop = true;

        loop {
            for line in (&mut lines).flatten() {
                match self.handle_line(&line) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Ok(false);
                    }
                    Err(err) => match err {
                        SyncUpdateError::MismatchedInit { .. }
                        | SyncUpdateError::MissingInitOptions => {
                            return Err(err);
                        }
                        _ => {
                            omni_warning!(format!("{}", err));
                        }
                    },
                }

                if !first_loop {
                    // Reset the last activity time for each message
                    // we read after the first loop
                    last_activity = StdInstant::now();
                }
            }

            if file.try_lock_exclusive().is_ok() {
                Fs4FileExt::unlock(file)?;

                // The process was completed
                break Ok(true);
            }

            if let Some(pid) = self.attached_pid {
                if shell_is_interactive() && last_activity.elapsed() > timeout_duration {
                    // Hide the progress handler if any
                    if let Some(handler) = &self.current_handler {
                        handler.hide();
                    }

                    // Suggest to kill the process if it seems to be hanging
                    if suggest_and_kill(pid) {
                        break Ok(false);
                    }

                    // Show the progress handler again if we pursued
                    if let Some(handler) = &self.current_handler {
                        handler.show();
                    }

                    // Reset the last activity time
                    last_activity = StdInstant::now();
                }
            }

            // Indicate that we are not in the first loop anymore
            first_loop = false;

            // Sleep for a short time before checking again
            std::thread::sleep(StdDuration::from_millis(100));
        }
    }

    fn handle_line(&mut self, line: &str) -> Result<bool, SyncUpdateError> {
        // Remove any character until the first }
        let line = line.trim_start_matches(|c| c != '{');
        // JSON deserialize the line into a SyncUpdateOperation object
        // If the line is not valid JSON, return an error
        let sync_update = serde_json::from_str::<SyncUpdateOperation>(line)?;
        match sync_update {
            SyncUpdateOperation::Init(init) => {
                if self.seen_init {
                    return Err(SyncUpdateError::AlreadyInit);
                }

                if let Some(pid) = init.pid() {
                    // The timeout duration after which we suggest to kill the process
                    let timeout = global_config().up_command.attach_kill_timeout;

                    // If the file last modified time is more than the configured
                    // timeout, offer to kill the process
                    if shell_is_interactive()
                        && self.since_modified > timeout
                        && suggest_and_kill(pid)
                    {
                        return Ok(false);
                    }
                }

                self.seen_init = true;
                self.attached_pid = init.pid();

                if let Some(ref expected_init) = self.expected_init {
                    if expected_init != &init {
                        return Err(SyncUpdateError::MismatchedInit {
                            actual: Box::new(init),
                            expected: Box::new(expected_init.clone()),
                        });
                    }
                    self.missing_options = !expected_init.options_difference(&init).is_empty();
                }
                omni_info!(format!(
                    "attaching to running {} operation",
                    init.name().light_yellow()
                ));
            }
            SyncUpdateOperation::Exit(exit_code) => {
                if self.missing_options && exit_code == 0 {
                    return Err(SyncUpdateError::MissingInitOptions);
                }
                exit(exit_code);
            }
            SyncUpdateOperation::OmniError(error) => {
                omni_error!(error);
            }
            SyncUpdateOperation::OmniWarning(warning) => {
                omni_warning!(warning);
            }
            SyncUpdateOperation::OmniInfo(info) => {
                omni_info!(info);
            }
            SyncUpdateOperation::Progress(progress) => {
                let need_new_handler = match self.current_handler_id {
                    Some(ref current_handler_id) => current_handler_id != progress.handler_id(),
                    _ => true,
                };

                if need_new_handler {
                    // Create a new handler for the new handler id
                    let new_handler = UpProgressHandler::new(progress.step());
                    new_handler.init(progress.desc().to_string());

                    self.current_handler = Some(new_handler);
                    self.current_handler_id = Some(progress.handler_id().to_string());
                }

                if let Some(ref mut handler) = self.current_handler {
                    handler.perform_sync_action(progress.action());
                } else {
                    return Err(SyncUpdateError::NoProgressHandler);
                }
            }
        }

        Ok(true)
    }
}

fn suggest_and_kill(pid: u32) -> bool {
    let question = requestty::Question::confirm("suggest_and_kill_process")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message(format!(
            "{} attached process {} seems to be hanging; do you want to kill it?",
            "omni:".light_cyan(),
            format!("{pid}").underline(),
        ))
        .default(true)
        .build();

    let kill_process = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::Bool(confirmed) => confirmed,
            _ => false,
        },
        Err(err) => {
            println!("{}", format!("[✘] {err:?}").red());
            false
        }
    };

    if !kill_process {
        return false;
    }

    // Kill the process
    let nixpid = nix::unistd::Pid::from_raw(pid as i32);
    if let Err(err) = nix::sys::signal::kill(nixpid, nix::sys::signal::SIGKILL) {
        omni_error!(format!("failed to kill process {}: {}", pid, err));
        false
    } else {
        omni_info!(format!("killed process {}", pid));
        true
    }
}

/// An operation that is sent to indicate the progress of the operation.
/// This will allow to replicate operations happening in the main process
/// to the attaching process, giving the same sense of progress to the user
/// even if his command is attaching to a background-running one.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncUpdateOperation {
    Init(SyncUpdateInit),
    Exit(i32),
    Progress(SyncUpdateProgress),
    #[serde(rename = "error")]
    OmniError(String),
    #[serde(rename = "warning")]
    OmniWarning(String),
    #[serde(rename = "info")]
    OmniInfo(String),
}

impl SyncUpdateOperation {
    pub fn dump_to_file(&self, mut file: &std::fs::File) -> Result<(), std::io::Error> {
        // Serialize the update to JSON in a single line
        let update_json = serde_json::to_string(self)?;

        // Add a line return at the end of the JSON
        let update_json = format!("{update_json}\n");

        // Write the JSON to the file
        file.write_all(update_json.as_bytes())?;

        Ok(())
    }
}

/// An initial operation that is sent to indicate which command we are
/// running. This will allow to know if we are running an `up` or `down`
/// command, and if we are running an `up` command, which options were
/// passed to it.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SyncUpdateInit {
    Up {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        #[serde(default, skip_serializing_if = "HashSet::is_empty")]
        options: HashSet<SyncUpdateInitOption>,
        cache: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
    },
    Down {
        cache: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
    },
}

impl SyncUpdateInit {
    pub fn name(&self) -> &str {
        match self {
            SyncUpdateInit::Up { .. } => "up",
            SyncUpdateInit::Down { .. } => "down",
        }
    }

    pub fn options(&self) -> HashSet<SyncUpdateInitOption> {
        match self {
            SyncUpdateInit::Up {
                commit: _commit,
                options,
                ..
            } => options.clone(),
            SyncUpdateInit::Down { .. } => HashSet::new(),
        }
    }

    pub fn options_difference(&self, other: &SyncUpdateInit) -> HashSet<SyncUpdateInitOption> {
        self.options()
            .difference(&other.options())
            .cloned()
            .collect()
    }

    pub fn pid(&self) -> Option<u32> {
        match self {
            SyncUpdateInit::Up { pid, .. } => *pid,
            SyncUpdateInit::Down { pid, .. } => *pid,
        }
    }
}

impl PartialEq for SyncUpdateInit {
    fn eq(&self, other: &Self) -> bool {
        // For Up, we don't care about the options, just the commit and cache
        match (self, other) {
            (
                SyncUpdateInit::Up {
                    commit: commit1,
                    cache: cache1,
                    ..
                },
                SyncUpdateInit::Up {
                    commit: commit2,
                    cache: cache2,
                    ..
                },
            ) => commit1 == commit2 && cache1 == cache2,
            (
                SyncUpdateInit::Down { cache: cache1, .. },
                SyncUpdateInit::Down { cache: cache2, .. },
            ) => cache1 == cache2,
            _ => false,
        }
    }
}

impl Display for SyncUpdateInit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncUpdateInit::Up {
                commit: None,
                options,
                ..
            } if options.is_empty() => write!(f, "up"),
            SyncUpdateInit::Up {
                commit: None,
                options,
                ..
            } => {
                write!(f, "up (options: {options:?})")
            }
            SyncUpdateInit::Up {
                commit: Some(commit),
                options,
                ..
            } if options.is_empty() => {
                write!(f, "up (commit: {commit})")
            }
            SyncUpdateInit::Up {
                commit: Some(commit),
                options,
                ..
            } => {
                write!(f, "up (commit: {commit}, options: {options:?})")
            }
            SyncUpdateInit::Down { .. } => write!(f, "down"),
        }
    }
}

/// A set of options for the `SyncUpdateInit::Up` variant, that allows
/// to indicate which options were passed to the `up` command. This will
/// enable to know if a command needs to go over the suggestions or if
/// nothing is left to do after synchronizing with a running process.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SyncUpdateInitOption {
    SuggestConfig,
    SuggestClone,
}

/// A progress update that is sent to indicate the progress of the
/// operation. This will allow to show a progress bar or a spinner
/// in the terminal, and to show the progress of the operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncUpdateProgress {
    #[serde(rename = "id", skip_serializing_if = "str::is_empty")]
    handler_id: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    desc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<(usize, usize)>,
    #[serde(flatten)]
    action: SyncUpdateProgressAction,
}

impl SyncUpdateProgress {
    pub fn handler_id(&self) -> &str {
        &self.handler_id
    }

    pub fn desc(&self) -> &str {
        &self.desc
    }

    pub fn step(&self) -> Option<(usize, usize)> {
        self.step
    }

    pub fn action(&self) -> &SyncUpdateProgressAction {
        &self.action
    }
}

/// An action that is sent to indicate the progress of the operation.
/// This will allow to update the shown progress bar or spinner in the
/// terminal.
#[derive(Debug)]
pub enum SyncUpdateProgressAction {
    Progress(String),
    Success(Option<String>),
    Error(Option<String>),
    Hide,
    Show,
    Println(String),
}

impl SyncUpdateProgressAction {
    fn from_map(map: BTreeMap<String, String>) -> Option<SyncUpdateProgressAction> {
        let action = map.get("action")?;

        match action.as_str() {
            "progress" => map
                .get("message")
                .map(|message| SyncUpdateProgressAction::Progress(message.clone())),
            "success" => {
                let message = map.get("message").cloned();
                Some(SyncUpdateProgressAction::Success(message))
            }
            "error" => {
                let message = map.get("message").cloned();
                Some(SyncUpdateProgressAction::Error(message))
            }
            "hide" => Some(SyncUpdateProgressAction::Hide),
            "show" => Some(SyncUpdateProgressAction::Show),
            "println" => map
                .get("message")
                .map(|message| SyncUpdateProgressAction::Println(message.clone())),
            _ => None,
        }
    }

    fn as_map(&self) -> BTreeMap<String, String> {
        let mut as_map = BTreeMap::new();
        match self {
            SyncUpdateProgressAction::Progress(message) => {
                as_map.insert("action".to_string(), "progress".to_string());
                as_map.insert("message".to_string(), message.clone());
            }
            SyncUpdateProgressAction::Success(message) => {
                as_map.insert("action".to_string(), "success".to_string());
                if let Some(message) = message {
                    as_map.insert("message".to_string(), message.clone());
                }
            }
            SyncUpdateProgressAction::Error(message) => {
                as_map.insert("action".to_string(), "error".to_string());
                if let Some(message) = message {
                    as_map.insert("message".to_string(), message.clone());
                }
            }
            SyncUpdateProgressAction::Hide => {
                as_map.insert("action".to_string(), "hide".to_string());
            }
            SyncUpdateProgressAction::Show => {
                as_map.insert("action".to_string(), "show".to_string());
            }
            SyncUpdateProgressAction::Println(message) => {
                as_map.insert("action".to_string(), "println".to_string());
                as_map.insert("message".to_string(), message.clone());
            }
        }
        as_map
    }
}

impl Serialize for SyncUpdateProgressAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.as_map().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SyncUpdateProgressAction {
    fn deserialize<D>(deserializer: D) -> Result<SyncUpdateProgressAction, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        // Deserialize the JSON value into a BTreeMap<String, String>
        let map = BTreeMap::<String, String>::deserialize(deserializer)?;

        // Convert the map into a SyncUpdateProgressAction using the from_map method
        match SyncUpdateProgressAction::from_map(map) {
            Some(action) => Ok(action),
            None => Err(serde::de::Error::custom("invalid SyncUpdateProgressAction")),
        }
    }
}
