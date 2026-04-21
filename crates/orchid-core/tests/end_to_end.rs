//! End-to-end wiring: parser → registry → dispatcher → history.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orchid_core::{
    parse_command_line_with_registry, Action, ActionContext, ActionDispatcher, ActionOutcome,
    CommandCategory, CommandDescriptor, CommandRegistry, EventBus, EventBusConfig, EventFilter,
    EventSource, HandlerPriority, HistoryRecorder, Result, TerminalInvocation,
};
use orchid_storage::{OrchidConfig, StateStore};
use parking_lot::RwLock;

#[derive(Debug, Clone)]
struct EchoEvent {
    message: String,
}

impl orchid_core::Event for EchoEvent {
    fn event_type() -> &'static str {
        "test.echo"
    }
}

struct EchoAction {
    message: String,
}

#[async_trait]
impl Action for EchoAction {
    fn id(&self) -> &'static str {
        "test.echo"
    }
    fn display_name_key(&self) -> &'static str {
        "test.echo.name"
    }
    fn command_text(&self) -> String {
        format!("orc test echo {:?}", self.message)
    }
    async fn execute(&self, ctx: &ActionContext) -> Result<ActionOutcome> {
        ctx.bus
            .publish_and_flush(
                EventSource::Command,
                EchoEvent {
                    message: self.message.clone(),
                },
            )
            .await?;
        Ok(ActionOutcome::ok_with_message(self.message.clone()))
    }
    fn target(&self) -> Option<String> {
        Some(self.message.clone())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_parsed_and_dispatched_end_to_end() {
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let storage = Arc::new(StateStore::open_in_memory("0.0-test").unwrap());
    let config = Arc::new(RwLock::new(OrchidConfig::default()));

    let registry = Arc::new(CommandRegistry::new());
    registry
        .register(
            CommandDescriptor {
                id: "test.echo".into(),
                display_name_key: "test.echo.name".into(),
                description_key: None,
                category: CommandCategory::Developer,
                default_shortcut: None,
                terminal_invocation: Some(TerminalInvocation {
                    verb: "test echo".into(),
                    args: Vec::new(),
                }),
                icon_name: None,
            },
            Arc::new(|parsed| {
                let message = parsed
                    .positional
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "hello".into());
                Ok(Box::new(EchoAction { message }) as Box<dyn Action>)
            }),
        )
        .unwrap();

    // Subscriber for the echo event.
    let (_handle, mut rx) = bus
        .subscribe(
            EventFilter::of_type("test.echo"),
            HandlerPriority::Normal,
        )
        .unwrap();

    let dispatcher = ActionDispatcher::new().with_middleware(Arc::new(HistoryRecorder::new(
        Arc::clone(&storage),
        true,
    )) as _);

    // Parse → registry → action
    let (parsed, descriptor) =
        parse_command_line_with_registry("orc test echo hello", &registry).unwrap();
    assert_eq!(descriptor.id, "test.echo");

    let action = registry.build_action(&descriptor.id, parsed).unwrap();

    let ctx = ActionContext::new(Arc::clone(&bus), Arc::clone(&storage), Arc::clone(&config));
    let outcome = dispatcher.dispatch(action, &ctx).await.unwrap();
    assert!(outcome.success);

    // Event was delivered.
    let env = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("should receive echo event")
        .expect("channel open");
    let payload = env.downcast::<EchoEvent>().expect("downcast");
    assert_eq!(payload.message, "hello");

    // History recorded one entry.
    let r = storage.read().unwrap();
    let recent = r.iter_history_recent(10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].action_id, "test.echo");
    assert_eq!(recent[0].command_text, "orc test echo \"hello\"");
    assert_eq!(recent[0].target.as_deref(), Some("hello"));
}
