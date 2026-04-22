//! Lifecycle state machine for widget instances.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use orchid_storage::LifecycleState;
use tracing::debug;

use crate::error::{Result, WidgetError};
use crate::events::WidgetLifecycleChanged;
use crate::widget::instance::{SharedInstance, WidgetInstanceRuntime};
use crate::widget::WidgetContext;

/// Drives transitions between [`LifecycleState`]s, invoking the widget's
/// `on_*` callbacks under its lock and broadcasting
/// [`WidgetLifecycleChanged`].
#[derive(Clone)]
pub struct LifecycleController {
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for LifecycleController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LifecycleController").finish_non_exhaustive()
    }
}

impl LifecycleController {
    /// Construct a controller bound to a bus for change notifications.
    #[must_use]
    pub fn new(bus: Arc<orchid_core::EventBus>) -> Self {
        Self { bus }
    }

    /// Transition an instance to `target`. No-op when the instance is
    /// already in that state.
    ///
    /// # Errors
    ///
    /// * [`WidgetError::InvalidStateForOperation`] for disallowed edges
    ///   in the state machine.
    /// * Bubbles up any error the widget's callback raised.
    pub async fn transition(
        &self,
        instance: &WidgetInstanceRuntime,
        ctx: &WidgetContext,
        target: LifecycleState,
    ) -> Result<()> {
        let from = *instance.lifecycle.read();
        if from == target {
            return Ok(());
        }
        if !is_allowed(from, target) {
            return Err(WidgetError::InvalidStateForOperation(format!(
                "{from:?} -> {target:?}"
            )));
        }

        {
            let mut widget = instance.widget.lock().await;
            match target {
                LifecycleState::Active => widget.on_activate(ctx).await?,
                LifecycleState::Sleeping => widget.on_sleep(ctx).await?,
                LifecycleState::Unloaded => widget.on_unload(ctx).await?,
            }
        }

        *instance.lifecycle.write() = target;
        *instance.updated_at.write() = Utc::now();

        self.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WidgetLifecycleChanged {
                instance_id: instance.id,
                from,
                to: target,
            },
        );
        debug!(instance_id = %instance.id, ?from, ?target, "lifecycle transition");
        Ok(())
    }

    /// Put every instance that has been idle longer than `idle` into
    /// `Sleeping`. Returns the number of transitions performed.
    ///
    /// # Errors
    ///
    /// Propagates whatever the per-instance transitions raise.
    pub async fn sleep_idle(
        &self,
        instances: &[SharedInstance],
        idle: Duration,
        ctx_for: &(dyn Fn(&WidgetInstanceRuntime) -> WidgetContext + Send + Sync),
    ) -> Result<usize> {
        let now = Utc::now();
        let mut count = 0;
        for instance in instances {
            let state = *instance.lifecycle.read();
            if state != LifecycleState::Active {
                continue;
            }
            let idle_for = now
                .signed_duration_since(*instance.last_touched.read())
                .to_std()
                .unwrap_or(Duration::ZERO);
            if idle_for >= idle {
                let ctx = ctx_for(instance);
                self.transition(instance, &ctx, LifecycleState::Sleeping).await?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Unload every `Sleeping` instance that has been sleeping for at least
    /// `since`.
    ///
    /// # Errors
    ///
    /// Propagates whatever the per-instance transitions raise.
    pub async fn unload_stale(
        &self,
        instances: &[SharedInstance],
        since: Duration,
        ctx_for: &(dyn Fn(&WidgetInstanceRuntime) -> WidgetContext + Send + Sync),
    ) -> Result<usize> {
        let now = Utc::now();
        let mut count = 0;
        for instance in instances {
            let state = *instance.lifecycle.read();
            if state != LifecycleState::Sleeping {
                continue;
            }
            let sleeping_for = now
                .signed_duration_since(*instance.updated_at.read())
                .to_std()
                .unwrap_or(Duration::ZERO);
            if sleeping_for >= since {
                let ctx = ctx_for(instance);
                self.transition(instance, &ctx, LifecycleState::Unloaded).await?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Edge rules for the lifecycle state machine.
fn is_allowed(from: LifecycleState, to: LifecycleState) -> bool {
    use LifecycleState::*;
    match (from, to) {
        (Active, Sleeping) | (Sleeping, Active) => true,
        (Sleeping, Unloaded) => true,
        (Unloaded, Active) => true,
        // Reactivating from Sleeping straight to Active is the common path.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_allowed_state_machine() {
        use LifecycleState::*;
        assert!(is_allowed(Active, Sleeping));
        assert!(is_allowed(Sleeping, Active));
        assert!(is_allowed(Sleeping, Unloaded));
        assert!(is_allowed(Unloaded, Active));
        assert!(!is_allowed(Active, Unloaded));
        assert!(!is_allowed(Unloaded, Sleeping));
    }
}
