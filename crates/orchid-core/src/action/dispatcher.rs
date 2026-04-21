//! Action dispatcher with middleware and panic-catching.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{info, warn};

use crate::action::context::{ActionContext, ActionOutcome};
use crate::action::Action;
use crate::error::{CoreError, Result};

/// Hook around action execution. Middlewares run on every dispatch.
///
/// `before` runs in registration order; `after` runs in **reverse**
/// registration order (LIFO) so that enclosing middleware can see the
/// outcome of inner middleware.
#[async_trait]
pub trait ActionMiddleware: Send + Sync + 'static {
    /// Called before the action executes. Returning an error aborts the
    /// dispatch without running the action or any later `before` hooks.
    async fn before(&self, action: &dyn Action, ctx: &ActionContext) -> Result<()>;

    /// Called after the action executes (or is aborted during `before`).
    async fn after(
        &self,
        action: &dyn Action,
        ctx: &ActionContext,
        outcome: &Result<ActionOutcome>,
    );
}

/// Executes actions and runs [`ActionMiddleware`] around them.
#[derive(Default, Clone)]
pub struct ActionDispatcher {
    middlewares: Vec<Arc<dyn ActionMiddleware>>,
}

impl std::fmt::Debug for ActionDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionDispatcher")
            .field("middleware_count", &self.middlewares.len())
            .finish()
    }
}

impl ActionDispatcher {
    /// Build an empty dispatcher.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::ActionDispatcher;
    /// let d = ActionDispatcher::new();
    /// let _ = d;
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append middleware. Returns `self` for fluent chaining.
    #[must_use]
    pub fn with_middleware(mut self, mw: Arc<dyn ActionMiddleware>) -> Self {
        self.middlewares.push(mw);
        self
    }

    /// Dispatch `action` through every middleware, catching panics.
    ///
    /// Order of operations:
    ///
    /// 1. Run every `before` in registration order. If any returns `Err`, the
    ///    action is skipped; `after` still runs for the middlewares that did
    ///    execute `before`, in reverse order, with the error as outcome.
    /// 2. Execute `action.execute`. Panics are converted to
    ///    [`CoreError::ActionFailed`].
    /// 3. Run every `after` in reverse registration order, passing the
    ///    outcome.
    ///
    /// # Errors
    ///
    /// Returns whatever error `before` or `execute` produced. `after`
    /// failures are logged and swallowed — the outcome returned is whatever
    /// the action produced.
    pub async fn dispatch(
        &self,
        action: Box<dyn Action>,
        ctx: &ActionContext,
    ) -> Result<ActionOutcome> {
        let action_id = action.id();
        let cmd_text = action.command_text();
        let corr = ctx.correlation_id;
        let span = tracing::info_span!("action.dispatch", action.id = action_id, correlation_id = ?corr);
        let _enter = span.enter();

        // 1) before middlewares. We track the index so that on failure we
        // can replay `after` only for the middlewares that actually ran
        // `before`.
        let mut before_reached = 0_usize;
        for (idx, mw) in self.middlewares.iter().enumerate() {
            if let Err(e) = mw.before(&*action, ctx).await {
                let err = Err::<ActionOutcome, _>(e);
                for mw_done in self.middlewares.iter().take(before_reached).rev() {
                    mw_done.after(&*action, ctx, &err).await;
                }
                return err;
            }
            before_reached = idx + 1;
        }

        // 2) execute with panic catch
        let outcome = run_with_panic_catch(&*action, ctx).await;

        match &outcome {
            Ok(o) => info!(
                action.id = action_id,
                success = o.success,
                "action completed"
            ),
            Err(e) => warn!(action.id = action_id, error = %e, "action failed"),
        }

        // 3) after middlewares in reverse order
        for mw in self.middlewares.iter().rev() {
            mw.after(&*action, ctx, &outcome).await;
        }

        // The `cmd_text` variable is threaded through the span purely for
        // debug builds so that richly-formatted logs can include it without
        // calling `command_text()` repeatedly.
        let _ = cmd_text;

        outcome
    }
}

/// Execute an action while capturing any panic into a [`CoreError::ActionFailed`].
///
/// We call `execute` directly on the current task rather than `tokio::spawn`,
/// which would require `'static` ownership of the action. Instead, we wrap
/// the future in [`AssertUnwindSafe`] + a poll shim that catches panics at
/// each poll.
async fn run_with_panic_catch(
    action: &dyn Action,
    ctx: &ActionContext,
) -> Result<ActionOutcome> {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// Future wrapper that catches panics from the inner future's `poll`.
    struct CatchUnwind<F>(F);

    impl<F: Future + Unpin> Future for CatchUnwind<F> {
        type Output = std::thread::Result<F::Output>;
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let pinned = Pin::new(&mut self.0);
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| pinned.poll(cx)));
            match result {
                Ok(Poll::Ready(v)) => Poll::Ready(Ok(v)),
                Ok(Poll::Pending) => Poll::Pending,
                Err(payload) => Poll::Ready(Err(payload)),
            }
        }
    }

    let fut = Box::pin(action.execute(ctx));
    match CatchUnwind(fut).await {
        Ok(Ok(outcome)) => Ok(outcome),
        Ok(Err(e)) => Err(e),
        Err(panic) => {
            let msg = panic_payload_to_string(&panic);
            Err(CoreError::ActionFailed(format!("panic: {msg}")))
        }
    }
}

fn panic_payload_to_string(p: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-stringifiable panic>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventBus, EventBusConfig};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct Recorder {
        before_tag: &'static str,
        after_tag: &'static str,
        log: Arc<parking_lot::Mutex<Vec<&'static str>>>,
        fail_before: bool,
    }

    #[async_trait]
    impl ActionMiddleware for Recorder {
        async fn before(&self, _: &dyn Action, _: &ActionContext) -> Result<()> {
            self.log.lock().push(self.before_tag);
            if self.fail_before {
                return Err(CoreError::ActionFailed("blocked".into()));
            }
            Ok(())
        }
        async fn after(
            &self,
            _: &dyn Action,
            _: &ActionContext,
            _: &Result<ActionOutcome>,
        ) {
            self.log.lock().push(self.after_tag);
        }
    }

    struct Noop;

    #[async_trait]
    impl Action for Noop {
        fn id(&self) -> &'static str {
            "test.noop"
        }
        fn display_name_key(&self) -> &'static str {
            "test.noop.name"
        }
        fn command_text(&self) -> String {
            "orc test noop".into()
        }
        async fn execute(&self, _ctx: &ActionContext) -> Result<ActionOutcome> {
            Ok(ActionOutcome::ok())
        }
    }

    struct Panicker;
    #[async_trait]
    impl Action for Panicker {
        fn id(&self) -> &'static str {
            "test.panic"
        }
        fn display_name_key(&self) -> &'static str {
            "test.panic.name"
        }
        fn command_text(&self) -> String {
            "orc test panic".into()
        }
        async fn execute(&self, _ctx: &ActionContext) -> Result<ActionOutcome> {
            panic!("boom");
        }
    }

    fn ctx() -> ActionContext {
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());
        let config =
            Arc::new(parking_lot::RwLock::new(orchid_storage::OrchidConfig::default()));
        ActionContext::new(bus, storage, config)
    }

    #[tokio::test]
    async fn middleware_after_runs_in_reverse_order() {
        let log: Arc<parking_lot::Mutex<Vec<&'static str>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));

        let d = ActionDispatcher::new()
            .with_middleware(Arc::new(Recorder {
                before_tag: "B1",
                after_tag: "A1",
                log: Arc::clone(&log),
                fail_before: false,
            }))
            .with_middleware(Arc::new(Recorder {
                before_tag: "B2",
                after_tag: "A2",
                log: Arc::clone(&log),
                fail_before: false,
            }));

        let _ = d.dispatch(Box::new(Noop), &ctx()).await.unwrap();
        assert_eq!(log.lock().clone(), vec!["B1", "B2", "A2", "A1"]);
    }

    #[tokio::test]
    async fn middleware_before_err_aborts_dispatch() {
        let log = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let d = ActionDispatcher::new()
            .with_middleware(Arc::new(Recorder {
                before_tag: "B1",
                after_tag: "A1",
                log: Arc::clone(&log),
                fail_before: false,
            }))
            .with_middleware(Arc::new(Recorder {
                before_tag: "B2-fail",
                after_tag: "A2",
                log: Arc::clone(&log),
                fail_before: true,
            }))
            .with_middleware(Arc::new(Recorder {
                before_tag: "B3",
                after_tag: "A3",
                log: Arc::clone(&log),
                fail_before: false,
            }));

        let err = d.dispatch(Box::new(Noop), &ctx()).await.unwrap_err();
        assert!(matches!(err, CoreError::ActionFailed(_)));
        let entries = log.lock().clone();
        // B1 ran, B2-fail ran (and failed). B3 did not run. `after` runs only
        // for middlewares that successfully ran `before` -> just A1.
        assert_eq!(entries, vec!["B1", "B2-fail", "A1"]);
    }

    #[tokio::test]
    async fn action_panic_surfaces_as_action_failed() {
        let d = ActionDispatcher::new();
        let err = d.dispatch(Box::new(Panicker), &ctx()).await.unwrap_err();
        match err {
            CoreError::ActionFailed(m) => assert!(m.contains("boom"), "got {m}"),
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_without_middleware_returns_outcome() {
        let d = ActionDispatcher::new();
        let o = d.dispatch(Box::new(Noop), &ctx()).await.unwrap();
        assert!(o.success);
    }

    #[tokio::test]
    async fn after_runs_even_when_execute_fails() {
        struct CounterMw(Arc<AtomicUsize>);

        #[async_trait]
        impl ActionMiddleware for CounterMw {
            async fn before(&self, _: &dyn Action, _: &ActionContext) -> Result<()> {
                Ok(())
            }
            async fn after(
                &self,
                _: &dyn Action,
                _: &ActionContext,
                _: &Result<ActionOutcome>,
            ) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let count = Arc::new(AtomicUsize::new(0));
        let d = ActionDispatcher::new().with_middleware(Arc::new(CounterMw(Arc::clone(&count))));
        let _ = d.dispatch(Box::new(Panicker), &ctx()).await;
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}
