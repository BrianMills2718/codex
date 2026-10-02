use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use std::collections::BTreeMap;

pub(crate) const MANAGE_HOOKS_TOOL_NAME: &str = "manage_hooks";

pub(crate) fn create_manage_hooks_tool() -> ToolSpec {
    let mut properties = BTreeMap::new();
    properties.insert(
        "action".to_string(),
        JsonSchema::string_enum(
            vec![serde_json::json!("list"), serde_json::json!("set_enabled")],
            Some("List hooks or enable/disable one hook.".to_string()),
        ),
    );
    properties.insert(
        "selector".to_string(),
        JsonSchema::string(Some(
            "Exact hook name or stable key returned by action=list. Required for set_enabled."
                .to_string(),
        )),
    );
    properties.insert(
        "enabled".to_string(),
        JsonSchema::boolean(Some("Desired state. Required for set_enabled.".to_string())),
    );

    ToolSpec::Function(ResponsesApiTool {
        name: MANAGE_HOOKS_TOOL_NAME.to_string(),
        description: "List lifecycle hooks by readable name or enable/disable an already-trusted hook. Changes apply to the running session and are persisted. This tool cannot trust new or modified hook commands."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["action".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}
