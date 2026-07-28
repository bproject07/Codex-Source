use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::TokenUsage;
use serde_json::Value;
use serde_json::json;

use crate::api_server_wire::ParsedChatRequest;
use crate::api_server_wire::ParsedRequest;

pub(crate) fn models_response(models: &[String], created: i64) -> Value {
    json!({
        "object": "list",
        "data": models
            .iter()
            .map(|id| json!({
                "id": id,
                "object": "model",
                "created": created,
                "owned_by": "openai",
            }))
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn responses_response(
    id: &str,
    model: &str,
    created_at: i64,
    request: &ParsedRequest,
    output: &[ResponseItem],
    usage: &TokenUsage,
) -> Value {
    let output = output
        .iter()
        .enumerate()
        .map(|(index, item)| response_item_value(item, &fallback_item_id(item, index), "completed"))
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "object": "response",
        "created_at": created_at,
        "completed_at": created_at,
        "status": "completed",
        "background": false,
        "error": null,
        "incomplete_details": null,
        "instructions": request.instructions,
        "max_output_tokens": null,
        "model": model,
        "output": output,
        "parallel_tool_calls": request.parallel_tool_calls,
        "previous_response_id": null,
        "reasoning": {
            "effort": request.reasoning_effort.as_ref().map(ReasoningEffort::as_str),
            "summary": null,
        },
        "service_tier": request.service_tier,
        "store": false,
        "temperature": null,
        "text": {"format": {"type": "text"}},
        "tool_choice": request.tool_choice,
        "tools": request.tools.clone().unwrap_or_default(),
        "top_p": null,
        "truncation": "disabled",
        "usage": responses_usage(usage),
        "user": null,
        "metadata": {},
    })
}

pub(crate) fn response_item_value(item: &ResponseItem, fallback_id: &str, status: &str) -> Value {
    match item {
        ResponseItem::Message {
            id, role, content, ..
        } => json!({
            "id": id.as_ref().map(ToString::to_string).unwrap_or_else(|| fallback_id.to_string()),
            "type": "message",
            "status": status,
            "role": role,
            "content": content.iter().filter_map(output_text).collect::<Vec<_>>(),
        }),
        ResponseItem::FunctionCall {
            id,
            name,
            arguments,
            call_id,
            ..
        } => json!({
            "id": id.as_ref().map(ToString::to_string).unwrap_or_else(|| fallback_id.to_string()),
            "type": "function_call",
            "status": status,
            "name": name,
            "arguments": arguments,
            "call_id": call_id,
        }),
        ResponseItem::Reasoning {
            id,
            summary,
            content,
            encrypted_content,
            ..
        } => json!({
            "id": id.as_ref().map(ToString::to_string).unwrap_or_else(|| fallback_id.to_string()),
            "type": "reasoning",
            "summary": summary,
            "content": content,
            "encrypted_content": encrypted_content,
        }),
        _ => {
            let mut value = serde_json::to_value(item).unwrap_or(Value::Null);
            if let Some(object) = value.as_object_mut() {
                object.remove("internal_chat_message_metadata_passthrough");
                object.entry("id").or_insert_with(|| json!(fallback_id));
            }
            value
        }
    }
}

pub(crate) fn response_item_started_value(item: &ResponseItem, fallback_id: &str) -> Value {
    let mut value = response_item_value(item, fallback_id, "in_progress");
    if let Some(object) = value.as_object_mut() {
        match item {
            ResponseItem::Message { .. } => {
                object.insert("content".to_string(), json!([]));
            }
            ResponseItem::FunctionCall { .. } => {
                object.insert("arguments".to_string(), json!(""));
            }
            _ => {}
        }
    }
    value
}

pub(crate) fn response_item_text(item: &ResponseItem) -> Option<String> {
    let ResponseItem::Message { content, .. } = item else {
        return None;
    };
    let text = content
        .iter()
        .filter_map(|part| match part {
            ContentItem::OutputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    Some(text)
}

pub(crate) fn responses_usage(usage: &TokenUsage) -> Value {
    json!({
        "input_tokens": usage.input_tokens,
        "input_tokens_details": {"cached_tokens": usage.cached_input_tokens},
        "output_tokens": usage.output_tokens,
        "output_tokens_details": {"reasoning_tokens": usage.reasoning_output_tokens},
        "total_tokens": usage.total_tokens,
    })
}

pub(crate) fn chat_response(
    id: &str,
    model: &str,
    created: i64,
    request: &ParsedChatRequest,
    output: &[ResponseItem],
    usage: &TokenUsage,
) -> Value {
    let mut text = String::new();
    let mut saw_message = false;
    let mut calls = Vec::new();
    for item in output {
        match item {
            ResponseItem::Message { content, .. } => {
                saw_message = true;
                for part in content {
                    if let ContentItem::InputText { text: delta }
                    | ContentItem::OutputText { text: delta } = part
                    {
                        text.push_str(delta);
                    }
                }
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => calls.push(json!({
                "id": call_id,
                "type": "function",
                "function": {"name": name, "arguments": arguments},
            })),
            _ => {}
        }
    }
    let finish_reason = if calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };
    json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": saw_message.then_some(text),
                "refusal": null,
                "annotations": [],
                "tool_calls": (!calls.is_empty()).then_some(calls),
            },
            "logprobs": null,
            "finish_reason": finish_reason,
        }],
        "usage": chat_usage(usage),
        "service_tier": request.request.service_tier,
        "system_fingerprint": null,
    })
}

pub(crate) fn chat_usage(usage: &TokenUsage) -> Value {
    json!({
        "prompt_tokens": usage.input_tokens,
        "prompt_tokens_details": {"cached_tokens": usage.cached_input_tokens},
        "completion_tokens": usage.output_tokens,
        "completion_tokens_details": {"reasoning_tokens": usage.reasoning_output_tokens},
        "total_tokens": usage.total_tokens,
    })
}

fn fallback_item_id(item: &ResponseItem, index: usize) -> String {
    let prefix = match item {
        ResponseItem::Message { .. } => "msg",
        ResponseItem::FunctionCall { .. } => "fc",
        ResponseItem::Reasoning { .. } => "rs",
        _ => "item",
    };
    format!("{prefix}_{index}")
}

fn output_text(content: &ContentItem) -> Option<Value> {
    match content {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => Some(json!({
            "type": "output_text",
            "text": text,
            "annotations": [],
            "logprobs": [],
        })),
        ContentItem::InputImage { .. } | ContentItem::InputAudio { .. } => None,
    }
}
