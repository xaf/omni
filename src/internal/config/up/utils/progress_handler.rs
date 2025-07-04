use std::io::Write;

use tempfile::NamedTempFile;
use time::format_description::well_known::Rfc3339;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::process::Command as TokioCommand;
use tokio::runtime::Runtime;
use tokio::time::Duration;

use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::UpError;
use crate::internal::user_interface::print::filter_control_characters;
use crate::internal::user_interface::StringColor;
use crate::omni_warning;

pub trait ProgressHandler: Send + Sync {
    fn println(&self, message: String);
    fn progress(&self, message: String);
    fn success(&self);
    fn success_with_message(&self, message: String);
    fn error(&self);
    fn error_with_message(&self, message: String);
    fn hide(&self);
    fn show(&self);
}

impl std::fmt::Debug for dyn ProgressHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "ProgressHandler")
    }
}

pub fn run_progress(
    process_command: &mut TokioCommand,
    progress_handler: Option<&dyn ProgressHandler>,
    run_config: RunConfig,
) -> Result<(), UpError> {
    let rt = Runtime::new().map_err(|err| UpError::Exec(err.to_string()))?;
    rt.block_on(async_run_progress_readblocks(
        process_command,
        |stdout, stderr, hide| {
            if let Some(progress_handler) = &progress_handler {
                match hide {
                    Some(true) => progress_handler.hide(),
                    Some(false) => progress_handler.show(),
                    None => {}
                }
                if let Some(stdout) = stdout {
                    progress_handler.progress(stdout);
                } else if let Some(stderr) = stderr {
                    progress_handler.progress(stderr);
                }
            }
        },
        run_config,
    ))
}

pub fn run_command_with_handler<F>(
    command: &mut TokioCommand,
    handler_fn: F,
    run_config: RunConfig,
) -> Result<(), UpError>
where
    F: FnMut(Option<String>, Option<String>),
{
    let rt = Runtime::new().unwrap();
    rt.block_on(async_run_progress_readlines(
        command, handler_fn, run_config,
    ))
}

pub fn get_command_output(
    process_command: &mut TokioCommand,
    run_config: RunConfig,
) -> std::io::Result<std::process::Output> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async_get_output(process_command, run_config))
}

async fn async_get_output(
    process_command: &mut TokioCommand,
    run_config: RunConfig,
) -> std::io::Result<std::process::Output> {
    let mut listener_manager = match run_config
        .listener_manager_for_command(process_command)
        .await
    {
        Ok(listener_manager) => listener_manager,
        Err(err) => {
            return Err(std::io::Error::other(err));
        }
    };
    listener_manager.start();

    process_command.kill_on_drop(true);
    let mut command = match process_command.spawn() {
        Ok(command) => command,
        Err(err) => {
            let _ = listener_manager.stop().await;
            return Err(err);
        }
    };

    let mut result = None;
    let mut stdout_vec = Vec::new();
    let mut stderr_vec = Vec::new();

    let (mut stdout_reader, mut stderr_reader) =
        match (command.stdout.take(), command.stderr.take()) {
            (Some(stdout), Some(stderr)) => (
                BufReader::new(stdout).lines(),
                BufReader::new(stderr).lines(),
            ),
            _ => {
                let _ = listener_manager.stop().await;
                return Err(std::io::Error::other("stdout or stderr missing"));
            }
        };

    let mut stdout_open = true;
    let mut stderr_open = true;

    loop {
        tokio::select! {
            stdout_line = stdout_reader.next_line() => {
                match stdout_line {
                    Ok(Some(line)) => {
                        stdout_vec.extend_from_slice(line.as_bytes());
                    }
                    Ok(None) => stdout_open = false,  // End of stdout stream
                    Err(err) => {
                        result = Some(Err(err));
                        break;
                    }
                }
            }
            stderr_line = stderr_reader.next_line() => {
                match stderr_line {
                    Ok(Some(line)) => {
                        stderr_vec.extend_from_slice(line.as_bytes());
                    }
                    Ok(None) => stderr_open = false,  // End of stderr stream
                    Err(err) => {
                        result = Some(Err(err));
                        break;
                    }
                }
            }
            Some((handler, _interactive)) = listener_manager.next() => {
                if let Err(err) = handler().await {
                    omni_warning!(format!("{}", err));
                }
            }
            Some(_) = async_timeout(&run_config) => {
                result = Some(Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout")));
                break;
            }
        }

        if !stdout_open && !stderr_open {
            break;
        }
    }

    // Close the listener
    if let Err(err) = listener_manager.stop().await {
        omni_warning!("{}", err);
    }

    if let Some(result) = result {
        return result;
    }

    match command.wait_with_output().await {
        Ok(output) => {
            let mut output = output;
            output.stdout = stdout_vec;
            output.stderr = stderr_vec;
            Ok(output)
        }
        Err(err) => Err(err),
    }
}

async fn async_timeout(run_config: &RunConfig) -> Option<()> {
    if let Some(timeout) = run_config.timeout() {
        tokio::time::sleep(timeout).await;
        Some(())
    } else {
        None
    }
}

async fn async_run_progress_readblocks<F>(
    process_command: &mut TokioCommand,
    handler_fn: F,
    run_config: RunConfig,
) -> Result<(), UpError>
where
    F: Fn(Option<String>, Option<String>, Option<bool>),
{
    let mut listener_manager = match run_config
        .listener_manager_for_command(process_command)
        .await
    {
        Ok(listener_manager) => listener_manager,
        Err(err) => {
            return Err(UpError::Exec(err));
        }
    };
    listener_manager.start();

    if let Ok(mut command) = process_command.spawn() {
        // Create a temporary file to store the output
        let log_file_prefix = format!(
            "omni-exec.{}.",
            time::OffsetDateTime::now_utc()
                .replace_nanosecond(0)
                .unwrap()
                .format(&Rfc3339)
                .expect("failed to format date")
                .replace(['-', ':'], ""), // Remove the dashes in the date and the colons in the time
        );
        let mut log_file = match NamedTempFile::with_prefix(log_file_prefix.as_str()) {
            Ok(file) => file,
            Err(err) => {
                return Err(UpError::Exec(err.to_string()));
            }
        };

        if let (Some(mut stdout), Some(mut stderr)) = (command.stdout.take(), command.stderr.take())
        {
            let mut stdout_buffer = [0; 1024];
            let mut stderr_buffer = [0; 1024];
            let mut last_read = std::time::Instant::now();

            let mut stdout_open = true;
            let mut stderr_open = true;

            loop {
                tokio::select! {
                    stdout_result = stdout.read(&mut stdout_buffer), if stdout_open => {
                        match stdout_result {
                            Ok(0) => stdout_open = false,  // End of stdout stream
                            Ok(n) => {
                                last_read = std::time::Instant::now();
                                let stdout_output = &stdout_buffer[..n];
                                log_file.write_all(stdout_output).unwrap();
                                if let Ok(stdout_str) = std::str::from_utf8(stdout_output) {
                                    for line in stdout_str.lines() {
                                        if line.is_empty() {
                                            continue;
                                        }
                                        handler_fn(Some(if run_config.strip_ctrl_chars {
                                            filter_control_characters(line)
                                        } else { line.to_string() }), None, None);
                                    }
                                }
                            }
                            Err(_err) => break,
                        }
                    }
                    stderr_result = stderr.read(&mut stderr_buffer), if stderr_open => {
                        match stderr_result {
                            Ok(0) => stderr_open = false,  // End of stderr stream
                            Ok(n) => {
                                last_read = std::time::Instant::now();
                                let stderr_output = &stderr_buffer[..n];
                                log_file.write_all(stderr_output).unwrap();
                                if let Ok(stderr_str) = std::str::from_utf8(stderr_output) {
                                    for line in stderr_str.lines() {
                                        if line.is_empty() {
                                            continue;
                                        }
                                        handler_fn(None, Some(if run_config.strip_ctrl_chars {
                                            filter_control_characters(line)
                                        } else { line.to_string() }), None);
                                    }
                                }
                            }
                            Err(_err) => break,
                        }
                    }
                    Some((handler, interactive)) = listener_manager.next() => {
                        if interactive {
                            handler_fn(None, None, Some(true));
                        }
                        if let Err(err) = handler().await {
                            handler_fn(None, Some(err.to_string()), None);
                        }
                        if interactive {
                            handler_fn(None, None, Some(false));
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if let Some(timeout) = run_config.timeout() {
                            if last_read.elapsed() > timeout {
                                if (command.kill().await).is_err() {
                                    // Nothing special to do, we're returning an error anyway
                                }
                                return Err(UpError::Timeout(format!("{:?}", process_command.as_std())));
                            }
                        }
                    }
                    _ = command.wait() => {
                        // The command has finished, we can stop reading
                        stdout_open = false;
                        stderr_open = false;
                    }
                }

                if !stdout_open && !stderr_open {
                    break;
                }
            }
        }

        // Close the listener
        if let Err(err) = listener_manager.stop().await {
            handler_fn(None, Some(err.to_string()), None);
        }

        match command.wait().await {
            Err(err) => Err(UpError::Exec(err.to_string())),
            Ok(exit_status) if !exit_status.success() => {
                let exit_code = exit_status.code().unwrap_or(-42);
                // TODO: the log file should be prefixed by the tmpdir_cleanup_prefix
                // by default and renamed when deciding to keep it
                match log_file.keep() {
                    Ok((_file, path)) => Err(UpError::Exec(format!(
                        "process exited with status {}; log is available at {}",
                        exit_code,
                        path.to_string_lossy().underline(),
                    ))),
                    Err(err) => Err(UpError::Exec(format!(
                        "process exited with status {exit_code}; failed to keep log file: {err}",
                    ))),
                }
            }
            Ok(_exit_status) => Ok(()),
        }
    } else {
        Err(UpError::Exec(format!("{:?}", process_command.as_std())))
    }
}

async fn async_run_progress_readlines<F>(
    process_command: &mut TokioCommand,
    mut handler_fn: F,
    run_config: RunConfig,
) -> Result<(), UpError>
where
    F: FnMut(Option<String>, Option<String>),
{
    let mut listener_manager = match run_config
        .listener_manager_for_command(process_command)
        .await
    {
        Ok(listener_manager) => listener_manager,
        Err(err) => {
            return Err(UpError::Exec(err));
        }
    };
    listener_manager.start();

    if let Ok(mut command) = process_command.spawn() {
        if let (Some(stdout), Some(stderr)) = (command.stdout.take(), command.stderr.take()) {
            let mut last_read = std::time::Instant::now();
            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut stderr_reader = BufReader::new(stderr).lines();

            let mut stdout_open = true;
            let mut stderr_open = true;

            loop {
                tokio::select! {
                    stdout_line = stdout_reader.next_line(), if stdout_open => {
                        match stdout_line {
                            Ok(Some(line)) => {
                                last_read = std::time::Instant::now();
                                listener_manager.recv_stdout(&line).await;
                                handler_fn(Some(if run_config.strip_ctrl_chars {
                                    filter_control_characters(&line)
                                } else { line }), None);

                            }
                            Ok(None) => stdout_open = false,  // End of stdout stream
                            Err(err) => return Err(UpError::Exec(err.to_string())),
                        }
                    }
                    stderr_line = stderr_reader.next_line(), if stderr_open => {
                        match stderr_line {
                            Ok(Some(line)) => {
                                last_read = std::time::Instant::now();
                                listener_manager.recv_stderr(&line).await;
                                handler_fn(None, Some(if run_config.strip_ctrl_chars {
                                    filter_control_characters(&line)
                                } else { line }));
                            }
                            Ok(None) => stderr_open = false,  // End of stderr stream
                            Err(err) => return Err(UpError::Exec(err.to_string())),
                        }
                    }
                    Some((handler, _interactive)) = listener_manager.next() => {
                        if let Err(err) = handler().await {
                            handler_fn(None, Some(err.to_string()));
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if let Some(timeout) = run_config.timeout() {
                            if last_read.elapsed() > timeout {
                                if (command.kill().await).is_err() {
                                    // Nothing special to do, we're returning an error anyway
                                }
                                return Err(UpError::Timeout(format!("{:?}", process_command.as_std())));
                            }
                        }
                    }
                    _ = command.wait() => {
                        // The command has finished, we can stop reading
                        stdout_open = false;
                        stderr_open = false;
                    }
                }

                if !stdout_open && !stderr_open {
                    break;
                }
            }
        }

        // Close the listener
        if let Err(err) = listener_manager.stop().await {
            handler_fn(None, Some(err.to_string()));
        }

        let exit_status = command.wait().await;
        if exit_status.is_err() || !exit_status.unwrap().success() {
            return Err(UpError::Exec(format!("{:?}", process_command.as_std())));
        }
    } else {
        return Err(UpError::Exec(format!("{:?}", process_command.as_std())));
    }

    Ok(())
}
