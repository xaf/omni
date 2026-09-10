//! Shared tokio runtime.
//!
//! omni is a short-lived, single-threaded CLI. Async is used in exactly one
//! way: to multiplex a handful of file descriptors while a child process runs
//! (subprocess stdout/stderr, and the askpass socket). That work is
//! cooperative and needs no worker threads.
//!
//! Before this module, seven call sites each built their own runtime with
//! `Runtime::new()`, which is `Builder::new_multi_thread()` -- so every one of
//! them spawned a worker thread per CPU. `run_progress` is called once per
//! command execution during `omni up`, so a build with many commands churned
//! through many multi-threaded runtimes to watch two pipes.
//!
//! This exposes a single, lazily built current-thread runtime instead. It is
//! safe because nothing in the codebase spawns tasks: there is no
//! `tokio::spawn` and no `spawn_blocking`, and all concurrency is
//! `tokio::select!` or `futures::{select_all, join_all}` *inside* a
//! `block_on`, which a current-thread runtime drives perfectly well.
//!
//! Dropping `rt-multi-thread` also lets the tokio feature list shrink.

use std::future::Future;

use once_cell::sync::Lazy;
use tokio::runtime::Builder;
use tokio::runtime::Runtime;

/// The process-wide runtime, built on first use and kept for the lifetime of
/// the process.
///
/// `enable_all()` is required: the time driver backs `tokio::time::timeout`,
/// and the I/O driver backs `tokio::process`, pipes and `UnixStream`.
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build the tokio runtime")
});

/// Run a future to completion on the shared runtime.
///
/// # Panics
///
/// Panics if called from inside a runtime context, which is tokio's own
/// behaviour for nested `block_on`. Callers that may already be inside a
/// runtime must check [`in_runtime`] first; see the `Drop` implementation of
/// `AskPassListener` for the pattern.
pub fn block_on<F: Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}

/// Whether the current thread is already inside a tokio runtime context.
///
/// Used to avoid a nested `block_on`, which would panic.
pub fn in_runtime() -> bool {
    tokio::runtime::Handle::try_current().is_ok()
}
