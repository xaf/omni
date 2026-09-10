use std::io::Write;

use tempfile::NamedTempFile;
use time::format_description::well_known::Rfc3339;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::process::Command as TokioCommand;
use tokio::time::Duration;

use crate::internal::config::up::utils::RunConfig;
use crate::internal::env::state_home;
use crate::internal::env::tmpdir_cleanup_prefix;
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

/// Directory holding logs kept from failed commands.
///
/// Under the state home rather than `$TMPDIR`: these are meant to outlive
/// the run that produced them, and `$TMPDIR` is reaped on a schedule nobody
/// controls.
pub fn logs_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(state_home()).join("logs")
}

/// Move the log of a failed command somewhere durable and findable.
///
/// While running, the log is a `NamedTempFile` carrying this run's
/// `tmpdir_cleanup` prefix, so it is reclaimable. Keeping it opts it out of
/// tempfile's Drop-based cleanup, which is precisely why it then has to be
/// moved: `NamedTempFile::keep` on its own is what leaked one permanent file
/// into `$TMPDIR` for every failed step, forever.
fn keep_log_file(log_file: NamedTempFile) -> Result<std::path::PathBuf, String> {
    keep_log_file_in(log_file, &logs_dir())
}

/// The body of [`keep_log_file`], with the destination injected so it can be
/// tested without touching the real state home.
fn keep_log_file_in(
    log_file: NamedTempFile,
    target_dir: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let (_file, tmp_path) = log_file.keep().map_err(|err| err.to_string())?;

    // Reuse the random component tempfile already picked, so two failures in
    // the same second cannot collide.
    let unique = tmp_path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.rsplit('.').next())
        .unwrap_or("log")
        .to_string();

    let timestamp = time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
        .replace(['-', ':'], "");

    if std::fs::create_dir_all(target_dir).is_err() {
        // Nowhere durable to put it. The log is still on disk where it was
        // written, so point at that rather than losing it outright.
        return Ok(tmp_path);
    }

    let target = target_dir.join(format!("omni-exec.{timestamp}.{unique}.log"));

    // rename() cannot cross filesystems, and $TMPDIR very often is one.
    if std::fs::rename(&tmp_path, &target).is_ok() {
        return Ok(target);
    }

    match std::fs::copy(&tmp_path, &target) {
        Ok(_) => {
            let _ = std::fs::remove_file(&tmp_path);
            Ok(target)
        }
        Err(_) => Ok(tmp_path),
    }
}

pub fn run_progress(
    process_command: &mut TokioCommand,
    progress_handler: Option<&dyn ProgressHandler>,
    run_config: RunConfig,
) -> Result<(), UpError> {
    crate::internal::utils::runtime::block_on(async_run_progress_readblocks(
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
    crate::internal::utils::runtime::block_on(async_run_progress_readlines(
        command, handler_fn, run_config,
    ))
}

pub fn get_command_output(
    process_command: &mut TokioCommand,
    run_config: RunConfig,
) -> std::io::Result<std::process::Output> {
    crate::internal::utils::runtime::block_on(async_get_output(process_command, run_config))
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
                    omni_warning!(err.to_string());
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
        // While the command is running its log lives in $TMPDIR under this
        // run's cleanup prefix, so tmpdir_cleanup() reclaims it if we exit
        // without dropping the file. It is only moved somewhere durable if
        // the command actually fails; see keep_log_file().
        let mut log_file = match NamedTempFile::with_prefix(tmpdir_cleanup_prefix("exec").as_str())
        {
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
                match keep_log_file(log_file) {
                    Ok(path) => Err(UpError::Exec(format!(
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

#[cfg(test)]
mod keep_log_file_tests {
    use std::io::Write;

    use super::*;

    /// Deliberately *not* using the tmpdir cleanup prefix: that namespace is
    /// process-global and shared with other modules, so a test creating
    /// files in it can be reaped by any other test that runs cleanup.
    fn temp_log(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("create temp log");
        file.write_all(contents.as_bytes()).expect("write log");
        file.flush().expect("flush log");
        file
    }

    /// Every failed step used to leak one permanent file into $TMPDIR,
    /// because `NamedTempFile::keep` opts the file out of Drop cleanup and
    /// nothing else ever removed it. It must end up somewhere durable and
    /// findable instead.
    #[test]
    fn a_kept_log_is_moved_into_the_logs_directory() {
        let dest = tempfile::tempdir().expect("dest dir");
        let log = temp_log("boom\n");
        let tmp_path = log.path().to_path_buf();

        let kept = keep_log_file_in(log, dest.path()).expect("keep log");

        assert!(kept.starts_with(dest.path()), "kept at {kept:?}");
        assert!(kept.exists(), "kept log should exist");
        assert!(
            !tmp_path.exists(),
            "the temporary copy must not be left behind"
        );
    }

    #[test]
    fn the_log_contents_survive_the_move() {
        let dest = tempfile::tempdir().expect("dest dir");
        let kept = keep_log_file_in(temp_log("stdout\nstderr\n"), dest.path()).expect("keep log");

        assert_eq!(
            std::fs::read_to_string(&kept).expect("read kept log"),
            "stdout\nstderr\n"
        );
    }

    /// Two steps failing within the same second must not overwrite each
    /// other's log, which is why the tempfile's random component is reused
    /// rather than relying on the timestamp alone.
    #[test]
    fn concurrent_failures_do_not_collide() {
        let dest = tempfile::tempdir().expect("dest dir");

        let first = keep_log_file_in(temp_log("first"), dest.path()).expect("keep first");
        let second = keep_log_file_in(temp_log("second"), dest.path()).expect("keep second");

        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second");
    }

    /// If the destination cannot be created the log must still be reported,
    /// not silently dropped -- a missing log is worse than one in $TMPDIR.
    #[test]
    fn an_unusable_destination_still_reports_a_readable_log() {
        let blocker = NamedTempFile::new().expect("blocker file");
        // A path *under a regular file* can never be created as a directory.
        let impossible = blocker.path().join("logs");

        let kept = keep_log_file_in(temp_log("still here"), &impossible).expect("keep log");

        assert!(kept.exists(), "log should still be readable at {kept:?}");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "still here");
    }

    /// The running log is named with this run's cleanup prefix so an
    /// abandoned one is reclaimable. That only works if cleanup removes
    /// plain files, which `remove_dir_all` does not.
    ///
    /// Exercises the per-entry helper rather than `tmpdir_cleanup()` itself:
    /// that reaps a process-global namespace shared with other modules, so
    /// calling it from a parallel test suite would delete other tests' files.
    #[test]
    fn cleanup_removes_both_files_and_directories() {
        let scratch = tempfile::tempdir().expect("scratch dir");

        let file = scratch.path().join("stray.log");
        std::fs::write(&file, "abandoned").expect("write stray log");

        let dir = scratch.path().join("stray-dir");
        std::fs::create_dir(&dir).expect("create stray dir");
        std::fs::write(dir.join("inner"), "x").expect("write inside stray dir");

        crate::internal::env::remove_cleanup_entry(&file);
        crate::internal::env::remove_cleanup_entry(&dir);

        assert!(!file.exists(), "a stray log file should be reclaimed");
        assert!(!dir.exists(), "a stray directory should still be reclaimed");
    }
}
