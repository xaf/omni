use std::fs::set_permissions;
use std::fs::Permissions;
use std::mem;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::pin::Pin;

use futures::Future;
use serde::Deserialize;
use serde::Serialize;
use shell_escape::escape;
use tempfile::TempDir;
use tera::Context;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::net::UnixStream;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex as TokioMutex;

use crate::internal::config::global_config;
use crate::internal::config::template::render_askpass_template;
use crate::internal::config::up::utils::force_remove_dir_all;
use crate::internal::config::up::utils::EventHandlerFn;
use crate::internal::config::up::utils::Listener;
use crate::internal::config::up::UpError;
use crate::internal::env::current_exe;
use crate::internal::env::shell_is_interactive;
use crate::internal::env::tmpdir_cleanup_prefix;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::user_interface::ensure_newline;

const ASKPASS_TOOLS: [&str; 2] = ["sudo", "ssh"];
const MAX_SOCKET_PATH_LEN: usize = 104;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AskPassRequest {
    #[serde(rename = "p", alias = "prompt")]
    prompt: String,
    #[serde(
        default,
        rename = "t",
        alias = "prompt_type",
        skip_serializing_if = "String::is_empty"
    )]
    prompt_type: String,
}

impl AskPassRequest {
    pub fn new(prompt: impl ToString, prompt_type: impl ToString) -> Self {
        Self {
            prompt: prompt.to_string(),
            prompt_type: prompt_type.to_string().to_lowercase(),
        }
    }

    pub fn send(&self, socket_path_str: &str) -> Result<String, String> {
        // Check if the file exists and is a socket
        let socket_path = PathBuf::from(socket_path_str);
        if !socket_path.exists() {
            return Err(format!("socket path does not exist: {socket_path_str}"));
        }

        let metadata = match socket_path.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                return Err(format!("error getting metadata for socket path: {err}"));
            }
        };

        if !metadata.file_type().is_socket() {
            return Err("socket path is not a socket".to_string());
        }

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(err) => {
                return Err(format!("error creating tokio runtime: {err}"));
            }
        };

        rt.block_on(async {
            let mut stream = match UnixStream::connect(socket_path).await {
                Ok(stream) => stream,
                Err(err) => {
                    return Err(format!("error connecting to socket: {err}"));
                }
            };

            // Serialize the request object to a string
            let request = serde_json::to_string(&self)
                .map_err(|err| format!("error serializing request: {err}"))?;
            // Make all bytes be the request + the 0 byte
            let request = format!("{request}\0");

            // Send the request through the socket
            if let Err(err) = stream.write_all(request.as_bytes()).await {
                return Err(format!("error writing to socket: {err}"));
            }

            // Wrap the socket in a BufReader for reading lines
            let mut reader = BufReader::new(&mut stream);

            // Read data from the socket
            let mut buf = String::new();
            let result = match reader.read_line(&mut buf).await {
                Ok(0) => {
                    // End of stream (connection closed by the other end)
                    Ok("".to_string())
                }
                Ok(_) => {
                    // We received some data, we can print it and exit
                    Ok(buf.trim().to_string())
                }
                Err(err) => {
                    // Error reading from the socket
                    Err(format!("error reading from socket: {err}"))
                }
            };

            // Close the socket
            drop(stream);

            result
        })
    }

    pub fn prompt(&self) -> String {
        if self.prompt.is_empty() {
            "Password:".to_string()
        } else {
            self.prompt.clone()
        }
    }
}

#[derive(Debug)]
struct AskPassListenerInner {
    listener: UnixListener,
    tmp_dir: TempDir,
}

#[derive(Debug)]
pub struct AskPassListener {
    inner: TokioMutex<AskPassListenerInner>,
}

impl Drop for AskPassListener {
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

impl Listener for AskPassListener {
    fn set_process_env<'a>(
        &'a self,
        process: &'a mut TokioCommand,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let needs_askpass = Self::needs_askpass();
            let lock = self.inner.lock().await;
            for tool in &needs_askpass {
                let askpass_path = Self::askpass_path(&lock.tmp_dir, tool);
                let askpass_path = askpass_path.to_string_lossy().to_string();
                process.env(format!("{}_ASKPASS", tool.to_uppercase()), &askpass_path);
            }

            process.env("SSH_ASKPASS_REQUIRE", "force");
            process.env_remove("DISPLAY");

            Ok(())
        })
    }

    fn next(&self) -> Pin<Box<dyn Future<Output = (EventHandlerFn, bool)> + Send + '_>> {
        // Create a stream copy that we can move into the future
        Box::pin(async move {
            // Accept a connection from the socket
            loop {
                let lock = self.inner.lock().await;
                match lock.listener.accept().await {
                    Ok((mut stream, _addr)) => {
                        // Create the handler function with the correct type
                        let handler: EventHandlerFn = Box::new(move || {
                            Box::pin(async move {
                                AskPassListener::handle_request(&mut stream).await?;
                                Ok(())
                            })
                        });
                        return (handler, true);
                    }
                    Err(_err) => {}
                }
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let mut lock = self.inner.lock().await;

            // Take ownership of the tmp_dir and handle cleanup
            let tmp_dir = mem::replace(&mut lock.tmp_dir, TempDir::new().unwrap());
            let tmp_dir_path = tmp_dir.path().to_path_buf();
            if let Err(_err) = tmp_dir.close() {
                if let Err(err) = force_remove_dir_all(tmp_dir_path) {
                    return Err(err.to_string());
                }
            }

            Ok(())
        })
    }
}

impl AskPassListener {
    fn has_askpass(tool: &str) -> bool {
        match std::env::var(format!("{}_ASKPASS", tool.to_uppercase())) {
            Ok(askpass) => !askpass.is_empty(),
            Err(_) => false,
        }
    }

    fn needs_askpass() -> Vec<&'static str> {
        ASKPASS_TOOLS
            .iter()
            .filter_map(|tool| match Self::has_askpass(tool) {
                true => None,
                false => Some(*tool),
            })
            .collect::<Vec<_>>()
    }

    fn askpass_path(tmp_dir: &TempDir, tool: &str) -> PathBuf {
        tmp_dir
            .path()
            .join(format!("{}-askpass.sh", tool.to_lowercase()))
    }

    pub async fn new(command: &str) -> Result<Option<Self>, UpError> {
        let config = global_config();
        if !config.askpass.enabled {
            return Ok(None);
        }

        let needs_askpass = Self::needs_askpass();
        if needs_askpass.is_empty() {
            return Ok(None);
        }

        // Create a temporary directory
        let tmp_dir = match tempfile::Builder::new()
            .prefix(&tmpdir_cleanup_prefix("askpass"))
            .tempdir()
        {
            Ok(tmp_dir) => tmp_dir,
            Err(err) => {
                return Err(UpError::Exec(
                    format!("failed to create temporary directory: {err:?}").to_string(),
                ))
            }
        };

        // Prepare the paths to the socket
        let socket_path = tmp_dir.path().join("socket");
        let (_short_tmpdir, socket_path) =
            if socket_path.to_string_lossy().to_string().len() > MAX_SOCKET_PATH_LEN {
                // This is a fallback in case the temp directory following the system's
                // temporary directory is too long. This is a rare case, but it can happen
                // on some systems with very long temporary directories.
                let short_tmpdir = tempfile::Builder::new()
                    .prefix(&tmpdir_cleanup_prefix("askpass"))
                    .tempdir_in("/tmp") // Use /tmp to avoid long paths
                    .map_err(|err| {
                        UpError::Exec(
                            format!("failed to create temporary directory: {err:?}").to_string(),
                        )
                    })?;

                let short_socket_path = short_tmpdir.path().join("socket");
                if short_socket_path.to_string_lossy().to_string().len() > MAX_SOCKET_PATH_LEN {
                    return Err(UpError::Exec("socket path is too long".to_string()));
                }

                (Some(short_tmpdir), short_socket_path)
            } else {
                (None, socket_path)
            };

        // Generate the script
        let mut context = Context::new();
        context.insert(
            "OMNI_BIN",
            &escape(std::borrow::Cow::Borrowed(
                current_exe().to_string_lossy().as_ref(),
            )),
        );
        context.insert("SOCKET_PATH", socket_path.to_string_lossy().as_ref());
        context.insert("INTERACTIVE", &shell_is_interactive());
        context.insert(
            "PREFER_GUI",
            &(config.askpass.prefer_gui && config.askpass.enable_gui),
        );
        context.insert("ENABLE_GUI", &config.askpass.enable_gui);
        context.insert("COMMAND", command);

        // Render the script for all the required askpass tools
        for tool in &needs_askpass {
            // Prepare the path to the askpass script
            let askpass_path = Self::askpass_path(&tmp_dir, tool);

            // Copy the context and add the tool name
            let mut context = context.clone();
            context.insert("TOOL", tool);

            // Render the script
            let script = render_askpass_template(&context).map_err(|err| {
                eprintln!("[✘] Failed to render askpass script for {tool}: {err:?}");
                UpError::Exec(format!("failed to render askpass script: {err:?}"))
            })?;

            // Write the script to the file
            if let Err(err) = std::fs::write(&askpass_path, script) {
                return Err(UpError::Exec(
                    format!("failed to write askpass script: {err:?}").to_string(),
                ));
            }

            // Make the file executable, but only for the owner
            let permissions = Permissions::from_mode(0o700);
            if let Err(err) = set_permissions(&askpass_path, permissions) {
                return Err(UpError::Exec(
                    format!("failed to set permissions on askpass script: {err:?}").to_string(),
                ));
            }
        }

        // Create the listener
        let listener = UnixListener::bind(&socket_path)
            .map_err(|err| UpError::Exec(format!("failed to bind to socket: {err:?}")))?;

        Ok(Some(Self {
            inner: TokioMutex::new(AskPassListenerInner { listener, tmp_dir }),
        }))
    }

    pub async fn handle_request(stream: &mut UnixStream) -> Result<(), String> {
        ensure_newline();

        // Read the request object from the stream, byte by byte because
        // the request is terminated by a null byte
        let mut buf = String::new();
        loop {
            let mut bytes = [0; 1];

            // Use select to expire after 1 second of inactivity
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                    return Err("timeout reading request from socket".to_string());
                }
                read = stream.read(&mut bytes) => {
                    match read {
                        Ok(0) => {
                            break;
                        }
                        Ok(_) => {
                            let byte = bytes[0];
                            if byte == b'\0' {
                                break;
                            }
                            buf.push(byte as char);
                        }
                        Err(err) => {
                            return Err(format!("failed to read request from socket: {err:?}"));
                        }
                    }
                }
            }
        }

        // Deserialize the request object
        let request: AskPassRequest = serde_json::from_str(&buf)
            .map_err(|err| format!("failed to parse request: {err:?}"))?;

        // Handle the request
        let password = if request.prompt_type == "none" {
            // Print the prompt
            eprintln!("{} {}", "!".green().bold(), request.prompt().bold());

            // Return an empty string if the prompt type is "none"
            "".to_string()
        } else {
            let question = requestty::Question::password("askpass_request")
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message(request.prompt())
                .build();

            match requestty::prompt_one(question) {
                Ok(answer) => match answer {
                    requestty::Answer::String(password) => password,
                    _ => return Err("no password provided".to_string()),
                },
                Err(err) => {
                    println!("{}", format!("[✘] {err:?}").red());
                    return Err("no password provided".to_string());
                }
            }
        };

        let future = stream.write_all(password.as_bytes());
        let result = future.await;
        match result {
            Ok(_) => {}
            Err(err) => {
                return Err(format!("failed to write to socket: {err:?}"));
            }
        }

        Ok(())
    }
}
