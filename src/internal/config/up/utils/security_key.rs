use std::pin::Pin;
use std::time::Duration;

use duct::cmd;
use futures::Future;
use tokio::process::Command as TokioCommand;
use tokio::sync::Notify;

use crate::internal::config::up::utils::EventHandlerFn;
use crate::internal::config::up::utils::Listener;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::shell_is_interactive;
use crate::internal::user_interface::colors::StringColor;

#[derive(Debug)]
pub struct SecurityKeyListener {
    sk_select_timestamp: Option<std::time::Instant>,
    notify: Notify,
    // progress_handler: Option<Box<dyn ProgressHandler>>,
}

impl Drop for SecurityKeyListener {
    fn drop(&mut self) {
        if let Err(_err) = tokio::runtime::Handle::try_current() {
            if let Ok(rt) = tokio::runtime::Runtime::new() {
                rt.block_on(async {
                    let _ = self.stop().await;
                });
            }
        }
    }
}

impl Listener for SecurityKeyListener {
    fn set_process_env(&self, process: &mut TokioCommand) -> Result<(), String> {
        let verbose_ssh_cmd = get_verbose_ssh_command(1);
        process.env("GIT_SSH_COMMAND", &verbose_ssh_cmd);
        eprintln!("Setting env: GIT_SSH_COMMAND={}", verbose_ssh_cmd);

        Ok(())
    }

    fn next(&mut self) -> Pin<Box<dyn Future<Output = (EventHandlerFn, bool)> + Send + '_>> {
        // Create a stream copy that we can move into the future
        Box::pin(async move {
            loop {
                eprintln!("Waiting on notification");
                // Wait for a notification
                self.notify.notified().await;

                // Read the timestamp
                let timestamp = match self.sk_select_timestamp {
                    Some(ts) => ts,
                    None => {
                        // If no timestamp, get to the next iteration
                        continue;
                    }
                };

                // Wait for the timeout
                let left_to_wait = timestamp.elapsed() - Self::DURATION_TO_WAIT;
                if left_to_wait > Duration::ZERO {
                    tokio::time::sleep(left_to_wait).await;
                }

                // Check the timestamp value again
                if self.sk_select_timestamp.is_none() {
                    // If the timestamp is None, get to the next iteration
                    continue;
                }

                // If the timestamp is still there, let's create a progress handler
                // to show that we are waiting on the security key
                self.show_progress_bar();
            }

            // We should never reach here
            unreachable!()
        })
    }

    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            self.hide_progress_bar();
            Ok(())
        })
    }

    fn recv_stderr(&mut self, stderr: &str) {
        eprintln!("HELLOOOOO");
        eprintln!("DEBUG: stderr: {}", stderr);
        let mut timestamp = self.sk_select_timestamp;
        if stderr.contains("sk_select_by_cred:") {
            eprintln!("HELLO1");
            timestamp = Some(std::time::Instant::now());
            self.notify.notify_one();
        } else if timestamp.is_some() {
            eprintln!("HELLO2");
            // Any other stderr message resets the timestamp
            timestamp = None;
            self.hide_progress_bar();
        }
    }
}

impl SecurityKeyListener {
    const DURATION_TO_WAIT: Duration = Duration::from_millis(400);

    pub fn new() -> Self {
        Self {
            sk_select_timestamp: None,
            notify: Notify::new(),
            // progress_handler: None,
        }
    }

    fn show_progress_bar(&mut self) {
        println!("{}", "Waiting on security key...".light_black().italic());
        // if self.progress_handler.is_some() {
        // // If the progress handler is already set, do nothing
        // return;
        // }

        // let desc = "Waiting on security key".light_black().italic();
        // let progress_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
        // Box::new(SpinnerProgressHandler::new(desc, None))
        // } else {
        // Box::new(PrintProgressHandler::new(desc, None))
        // };

        // progress_handler.progress("".to_string());

        // self.progress_handler = Some(progress_handler);
    }

    fn hide_progress_bar(&mut self) {
        // if let Some(handler) = self.progress_handler.take() {
        // handler.success_with_message("".to_string());
        // handler.hide();
        // }
    }
}

fn get_verbose_ssh_command(level: usize) -> String {
    // Prepare an environment variable to override
    // the ssh command used by git
    let mut ssh_command = "ssh".to_string();

    if let Some(env_ssh_command) =
        std::env::var_os("GIT_SSH_COMMAND").and_then(|s| s.into_string().ok())
    {
        ssh_command = env_ssh_command.trim().to_string();
    } else if let Ok(cfg_ssh_command) = cmd!("git", "config", "--get", "core.sshCommand").read() {
        let cfg_ssh_command = cfg_ssh_command.trim();
        if !cfg_ssh_command.is_empty() {
            ssh_command = cfg_ssh_command.to_string();
        }
    };

    if level < 1 {
        return ssh_command;
    }

    // Add a number of 'v' that corresponds to the verbosity level
    format!("{} -{}", ssh_command, "v".repeat(level))
}
