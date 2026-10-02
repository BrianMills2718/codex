use codex_hooks::HookListEntry;
use codex_hooks::HookListEntryHandler;
use codex_protocol::protocol::HookTrustStatus;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde::Deserialize;
use serde::Serialize;
use toml_edit::value;

use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::manage_hooks_spec::MANAGE_HOOKS_TOOL_NAME;
use crate::tools::handlers::manage_hooks_spec::create_manage_hooks_tool;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;

#[derive(Debug, Deserialize)]
struct ManageHooksArgs {
    action: ManageHooksAction,
    selector: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManageHooksAction {
    List,
    SetEnabled,
}

#[derive(Debug, Serialize)]
struct HookSummary {
    name: String,
    key: String,
    event: String,
    enabled: bool,
    trust: String,
    managed: bool,
}

pub struct ManageHooksHandler;

impl ToolExecutor<ToolInvocation> for ManageHooksHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(MANAGE_HOOKS_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_manage_hooks_tool()
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        false
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(self.handle_call(invocation))
    }
}

impl ManageHooksHandler {
    async fn handle_call(
        &self,
        invocation: ToolInvocation,
    ) -> Result<Box<dyn crate::tools::context::ToolOutput>, FunctionCallError> {
        let ToolInvocation {
            payload, session, ..
        } = invocation;
        let ToolPayload::Function { arguments } = payload else {
            return Err(FunctionCallError::RespondToModel(
                "manage_hooks received unsupported payload".to_string(),
            ));
        };
        let args: ManageHooksArgs = parse_arguments(&arguments)?;
        let hooks = session.list_hooks().await;

        match args.action {
            ManageHooksAction::List => hook_output(&hooks.hooks),
            ManageHooksAction::SetEnabled => {
                let selector = args.selector.as_deref().ok_or_else(|| {
                    FunctionCallError::RespondToModel(
                        "selector is required for action=set_enabled".to_string(),
                    )
                })?;
                let enabled = args.enabled.ok_or_else(|| {
                    FunctionCallError::RespondToModel(
                        "enabled is required for action=set_enabled".to_string(),
                    )
                })?;
                let hook = select_hook(&hooks.hooks, selector)?;
                if hook.is_managed {
                    return Err(FunctionCallError::RespondToModel(format!(
                        "hook `{}` is managed and cannot be changed",
                        hook_display_name(hook)
                    )));
                }
                if !matches!(hook.trust_status, HookTrustStatus::Trusted) {
                    return Err(FunctionCallError::RespondToModel(format!(
                        "hook `{}` is new or modified; a human must review and trust it in /hooks",
                        hook_display_name(hook)
                    )));
                }

                let config = session.get_config().await;
                ConfigEditsBuilder::for_config(config.as_ref())
                    .with_edits([ConfigEdit::SetPath {
                        segments: vec![
                            "hooks".to_string(),
                            "state".to_string(),
                            hook.key.clone(),
                            "enabled".to_string(),
                        ],
                        value: value(enabled),
                    }])
                    .apply()
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!(
                            "failed to persist hook setting: {err}"
                        ))
                    })?;
                session.reload_user_config_layer().await;
                let refreshed = session.list_hooks().await;
                let updated = refreshed
                    .hooks
                    .iter()
                    .find(|candidate| candidate.key == hook.key)
                    .ok_or_else(|| {
                        FunctionCallError::RespondToModel(
                            "hook disappeared after configuration reload".to_string(),
                        )
                    })?;
                if updated.enabled != enabled {
                    return Err(FunctionCallError::RespondToModel(format!(
                        "hook reload did not apply the requested enabled={enabled} state"
                    )));
                }
                hook_output(std::slice::from_ref(updated))
            }
        }
    }
}

fn select_hook<'a>(
    hooks: &'a [HookListEntry],
    selector: &str,
) -> Result<&'a HookListEntry, FunctionCallError> {
    let matches = hooks
        .iter()
        .filter(|hook| hook.key == selector || hook_display_name(hook) == selector)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [hook] => Ok(*hook),
        [] => Err(FunctionCallError::RespondToModel(format!(
            "no hook matched `{selector}`; call manage_hooks with action=list"
        ))),
        _ => Err(FunctionCallError::RespondToModel(format!(
            "hook name `{selector}` is ambiguous; retry with its stable key"
        ))),
    }
}

fn hook_output(
    hooks: &[HookListEntry],
) -> Result<Box<dyn crate::tools::context::ToolOutput>, FunctionCallError> {
    let summaries = hooks
        .iter()
        .map(|hook| HookSummary {
            name: hook_display_name(hook),
            key: hook.key.clone(),
            event: format!("{:?}", hook.event_name),
            enabled: hook.enabled,
            trust: format!("{:?}", hook.trust_status),
            managed: hook.is_managed,
        })
        .collect::<Vec<_>>();
    let text = serde_json::to_string_pretty(&summaries).map_err(|err| {
        FunctionCallError::RespondToModel(format!("failed to serialize hook list: {err}"))
    })?;
    Ok(boxed_tool_output(FunctionToolOutput::from_text(
        text,
        Some(true),
    )))
}

pub(crate) fn hook_display_name(hook: &HookListEntry) -> String {
    if let Some(status_message) = hook
        .status_message
        .as_deref()
        .map(str::trim)
        .filter(|status_message| !status_message.is_empty())
    {
        return status_message.to_string();
    }
    match &hook.handler {
        HookListEntryHandler::Command { command, .. } => command
            .split_whitespace()
            .rev()
            .find_map(|token| {
                let token = token.trim_matches(['\'', '"']);
                let path = std::path::Path::new(token);
                path.file_stem()
                    .filter(|_| path.extension().is_some())
                    .and_then(std::ffi::OsStr::to_str)
                    .map(|stem| stem.replace(['_', '-'], " "))
            })
            .unwrap_or_else(|| "Command hook".to_string()),
        HookListEntryHandler::McpTool { server, tool } => format!("{server}.{tool}"),
    }
}

impl CoreToolRuntime for ManageHooksHandler {}

#[cfg(test)]
#[path = "manage_hooks_tests.rs"]
mod tests;
