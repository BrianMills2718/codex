use super::*;
use codex_protocol::protocol::HookEventName;
use codex_protocol::protocol::HookSource;

fn hook(status_message: Option<&str>, command: &str) -> HookListEntry {
    HookListEntry {
        builtin: false,
        key: "plugin:stop:0:0".to_string(),
        event_name: HookEventName::Stop,
        handler: HookListEntryHandler::Command {
            command: command.to_string(),
            r#async: false,
        },
        matcher: None,
        timeout_sec: 10,
        status_message: status_message.map(ToOwned::to_owned),
        additional_context_limit: None,
        source_path: codex_utils_absolute_path::AbsolutePathBuf::try_from(
            std::path::PathBuf::from("/tmp/hooks.json"),
        )
        .expect("absolute hook path"),
        source: HookSource::Plugin,
        plugin_id: Some("company-planning@inside-success".to_string()),
        display_order: 0,
        enabled: true,
        is_managed: false,
        current_hash: "sha256:test".to_string(),
        trust_status: HookTrustStatus::Trusted,
    }
}

#[test]
fn display_name_prefers_status_message() {
    let hook = hook(
        Some("Checking company-planning obligations"),
        "python3 hooks/planning_lifecycle_gate.py",
    );
    assert_eq!(
        hook_display_name(&hook),
        "Checking company-planning obligations"
    );
}

#[test]
fn display_name_falls_back_to_script_name() {
    let hook = hook(None, "python3 hooks/planning_lifecycle_gate.py --from-hook");
    assert_eq!(hook_display_name(&hook), "planning lifecycle gate");
}

#[test]
fn duplicate_names_require_stable_key() {
    let hooks = vec![
        hook(Some("Same name"), "one.py"),
        hook(Some("Same name"), "two.py"),
    ];
    let err = select_hook(&hooks, "Same name").expect_err("name should be ambiguous");
    assert!(err.to_string().contains("ambiguous"));
}
