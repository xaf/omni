use std::pin::Pin;
use std::time::Duration;

use duct::cmd;
use futures::Future;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Notify;

use crate::internal::config::up::utils::EventHandlerFn;
use crate::internal::config::up::utils::Listener;
use crate::internal::config::up::utils::PrintProgressHandler;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::SpinnerProgressHandler;
use crate::internal::env::shell_is_interactive;
use crate::internal::user_interface::colors::StringColor;

#[derive(Debug)]
struct SecurityKeyListenerInner {
    sk_select_timestamp: Option<std::time::Instant>,
    progress_handler: Option<Box<dyn ProgressHandler>>,
}

#[derive(Debug)]
pub struct SecurityKeyListener {
    notify: Notify,
    inner: TokioMutex<SecurityKeyListenerInner>,
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
    fn set_process_env<'a>(
        &'a self,
        process: &'a mut TokioCommand,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let verbose_ssh_cmd = get_verbose_ssh_command(1);
            process.env("GIT_SSH_COMMAND", &verbose_ssh_cmd);

            Ok(())
        })
    }

    fn next(&self) -> Pin<Box<dyn Future<Output = (EventHandlerFn, bool)> + Send + '_>> {
        // Create a stream copy that we can move into the future
        Box::pin(async move {
            loop {
                // Wait for a notification
                self.notify.notified().await;

                // Read the timestamp
                let timestamp = match self.read_timestamp().await {
                    Some(ts) => ts,
                    None => {
                        // If no timestamp, get to the next iteration
                        continue;
                    }
                };

                // Wait for the timeout
                if timestamp.elapsed() < Self::DURATION_TO_WAIT {
                    let left_to_wait = Self::DURATION_TO_WAIT - timestamp.elapsed();
                    tokio::time::sleep(left_to_wait).await;
                }

                // Check the timestamp value again
                if self.read_timestamp().await.is_none() {
                    // If the timestamp is None, get to the next iteration
                    continue;
                }

                // If the timestamp is still there, let's create a progress handler
                // to show that we are waiting on the security key
                self.show_progress_bar().await;
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            self.hide_progress_bar().await;
            Ok(())
        })
    }

    fn recv_stderr<'a>(&'a self, stderr: &'a str) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if stderr.contains("sk_select_by_cred:") {
                // Set the timestamp to the current time
                self.write_timestamp(Some(std::time::Instant::now())).await;
                self.notify.notify_one();
            } else if self.read_timestamp().await.is_some() {
                // Any other stderr message resets the timestamp
                self.write_timestamp(None).await;
                self.hide_progress_bar().await;
            }
        })
    }
}

impl SecurityKeyListener {
    const DURATION_TO_WAIT: Duration = Duration::from_millis(400);

    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
            inner: TokioMutex::new(SecurityKeyListenerInner {
                sk_select_timestamp: None,
                progress_handler: None,
            }),
        }
    }

    async fn read_timestamp(&self) -> Option<std::time::Instant> {
        let lock = self.inner.lock().await;
        lock.sk_select_timestamp
    }

    async fn write_timestamp(&self, timestamp: Option<std::time::Instant>) {
        let mut lock = self.inner.lock().await;
        lock.sk_select_timestamp = timestamp;
    }

    async fn show_progress_bar(&self) {
        let mut lock = self.inner.lock().await;

        if lock.progress_handler.is_some() {
            // If the progress handler is already set, do nothing
            return;
        }

        let desc = "Waiting on security key".light_black().italic();
        let progress_handler: Box<dyn ProgressHandler> = if shell_is_interactive() {
            Box::new(SpinnerProgressHandler::new(desc, None))
        } else {
            Box::new(PrintProgressHandler::new(desc, None))
        };

        progress_handler.progress("".to_string());

        lock.progress_handler = Some(progress_handler);
    }

    async fn hide_progress_bar(&self) {
        let mut lock = self.inner.lock().await;
        if let Some(handler) = lock.progress_handler.take() {
            handler.success_with_message("".to_string());
            handler.hide();
        }
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
