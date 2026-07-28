use std::collections::HashSet;

use codex_protocol::models::ResponseItem;

use crate::api_server_wire::ApiError;

pub(crate) fn validate_tool_call_sequence(
    items: &[ResponseItem],
    param: &str,
) -> Result<(), ApiError> {
    let mut calls = HashSet::new();
    let mut outputs = HashSet::new();
    for item in items {
        match item {
            ResponseItem::FunctionCall { call_id, .. }
                if (call_id.trim().is_empty() || !calls.insert(call_id.as_str())) =>
            {
                return Err(ApiError::invalid(
                    "Function call IDs must be non-empty and unique.",
                    Some(param),
                ));
            }
            ResponseItem::FunctionCallOutput { call_id, .. } => {
                if !calls.contains(call_id.as_str()) {
                    return Err(ApiError::invalid(
                        format!("Tool output references unknown call ID '{call_id}'."),
                        Some(param),
                    ));
                }
                if !outputs.insert(call_id.as_str()) {
                    return Err(ApiError::invalid(
                        format!("Duplicate tool output for call ID '{call_id}'."),
                        Some(param),
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
