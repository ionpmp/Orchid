//! Bulk shortcut-override application.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::{
    Action, ActionContext, ActionOutcome, CommandCategory, CommandDescriptor, CommandRegistry,
    ParsedCommand, Result, Shortcut,
};

struct Noop;
#[async_trait]
impl Action for Noop {
    fn id(&self) -> &'static str {
        "stub"
    }
    fn display_name_key(&self) -> &'static str {
        "stub.name"
    }
    fn command_text(&self) -> String {
        "orc stub".into()
    }
    async fn execute(&self, _: &ActionContext) -> Result<ActionOutcome> {
        Ok(ActionOutcome::ok())
    }
}

fn factory() -> orchid_core::ActionFactory {
    Arc::new(|_: ParsedCommand| Ok(Box::new(Noop) as Box<dyn Action>))
}

fn desc(id: &str, default_shortcut: Option<&str>) -> CommandDescriptor {
    CommandDescriptor {
        id: id.into(),
        display_name_key: format!("{id}.name"),
        description_key: None,
        category: CommandCategory::Developer,
        default_shortcut: default_shortcut.map(|s| Shortcut::parse(s).unwrap()),
        terminal_invocation: None,
        icon_name: None,
    }
}

#[test]
fn per_entry_outcomes_reported() {
    let reg = CommandRegistry::new();
    reg.register(desc("cmd.one", Some("Ctrl+1")), factory()).unwrap();
    reg.register(desc("cmd.two", Some("Ctrl+2")), factory()).unwrap();
    reg.register(desc("cmd.three", Some("Ctrl+3")), factory()).unwrap();

    let mut overrides = HashMap::new();
    overrides.insert("cmd.one".to_string(), "Ctrl+Shift+1".to_string());
    overrides.insert("cmd.two".to_string(), "not a shortcut".to_string());
    overrides.insert("cmd.three".to_string(), "Ctrl+Shift+3".to_string());
    overrides.insert("cmd.unknown".to_string(), "Ctrl+9".to_string());

    let results = reg.apply_shortcut_overrides(&overrides);
    assert_eq!(results.len(), 4);

    let map: HashMap<_, _> = results
        .into_iter()
        .map(|r| (r.command_id, r.outcome))
        .collect();
    assert!(map["cmd.one"].is_ok());
    assert!(map["cmd.two"].is_err());
    assert!(map["cmd.three"].is_ok());
    assert!(map["cmd.unknown"].is_err());

    assert_eq!(
        reg.effective_shortcut("cmd.one"),
        Some(Shortcut::parse("Ctrl+Shift+1").unwrap())
    );
    assert_eq!(
        reg.effective_shortcut("cmd.two"),
        Some(Shortcut::parse("Ctrl+2").unwrap()),
        "failed override must leave the default in place"
    );
    assert_eq!(
        reg.effective_shortcut("cmd.three"),
        Some(Shortcut::parse("Ctrl+Shift+3").unwrap())
    );
}
