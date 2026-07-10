//! Helpers for spawning work on the Slint event loop.

use std::future::Future;
use tracing::warn;

/// Spawn `fut` on the Slint UI thread; log if the event loop rejects it.
pub(crate) fn spawn_local(fut: impl Future<Output = ()> + 'static) {
    if let Err(e) = slint::spawn_local(fut) {
        warn!(error = %e, "slint::spawn_local failed to schedule");
    }
}

/// Like [`spawn_local`], wrapping with `async_compat::Compat` for tokio futures.
pub(crate) fn spawn_local_compat(fut: impl Future<Output = ()> + 'static) {
    spawn_local(async_compat::Compat::new(fut));
}

/// Run `bg` on the tokio thread pool, then run `cont` on the Slint UI thread.
///
/// Keeps the Slint event loop responsive: Compat yields while the background
/// task runs, instead of polling FS / decode work on the UI thread.
pub(crate) fn spawn_bg_then_local<T, Bg, ContFut, Cont>(bg: Bg, cont: Cont)
where
    T: Send + 'static,
    Bg: Future<Output = T> + Send + 'static,
    ContFut: Future<Output = ()> + 'static,
    Cont: FnOnce(T) -> ContFut + 'static,
{
    spawn_local_compat(async move {
        match tokio::task::spawn(bg).await {
            Ok(value) => cont(value).await,
            Err(e) => warn!(error = %e, "background UI task join failed"),
        }
    });
}

/// Spawn a fallible async task; log `context` + error if the future returns Err.
#[allow(dead_code)] // gradual migration from bare `let _ = slint::spawn_local`
pub(crate) fn spawn_local_result<E>(
    context: &'static str,
    fut: impl Future<Output = Result<(), E>> + 'static,
) where
    E: std::fmt::Display + 'static,
{
    spawn_local(async move {
        if let Err(e) = fut.await {
            warn!(error = %e, context, "async UI task failed");
        }
    });
}

#[allow(dead_code)] // gradual migration from bare `let _ = slint::spawn_local`
pub(crate) fn spawn_local_compat_result<E>(
    context: &'static str,
    fut: impl Future<Output = Result<(), E>> + 'static,
) where
    E: std::fmt::Display + 'static,
{
    spawn_local_compat(async move {
        if let Err(e) = fut.await {
            warn!(error = %e, context, "async UI task failed");
        }
    });
}
