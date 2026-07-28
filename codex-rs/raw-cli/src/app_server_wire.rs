use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use codex_protocol::protocol::TokenUsage;
use serde_json::Value;
use serde_json::json;

#[derive(Debug, PartialEq)]
pub(crate) struct RpcMessage {
    pub(crate) id: Option<Value>,
    pub(crate) method: String,
    pub(crate) params: Value,
}

#[derive(Debug)]
pub(crate) struct TurnInput {
    pub(crate) thread_id: String,
    pub(crate) prompt: String,
    pub(crate) wire_input: Value,
    pub(crate) client_id: Option<String>,
    pub(crate) model: Option<String>,
}

pub(crate) fn parse_message(line: &str) -> std::result::Result<RpcMessage, String> {
    let value: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "request must be a JSON object".to_string())?;
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| "request method must be a string".to_string())?;
    let id = object.get("id").cloned();
    if id
        .as_ref()
        .is_some_and(|id| !id.is_string() && !id.is_number())
    {
        return Err("request id must be a string or number".to_string());
    }
    Ok(RpcMessage {
        id,
        method: method.to_string(),
        params: object.get("params").cloned().unwrap_or_else(|| json!({})),
    })
}

pub(crate) fn parse_turn_input(params: &Value) -> std::result::Result<TurnInput, String> {
    let params = params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())?;
    let thread_id = required_string(params.get("threadId"), "threadId")?;
    let input = params
        .get("input")
        .and_then(Value::as_array)
        .ok_or_else(|| "input must be an array".to_string())?;
    if input.len() != 1 {
        return Err("Raw accepts exactly one text input".to_string());
    }
    let item = input[0]
        .as_object()
        .ok_or_else(|| "input item must be an object".to_string())?;
    if item.get("type").and_then(Value::as_str) != Some("text") {
        return Err("Raw accepts only text input".to_string());
    }
    let prompt = required_string(item.get("text"), "input text")?;
    if prompt.trim().is_empty() {
        return Err("input text must not be empty".to_string());
    }

    Ok(TurnInput {
        thread_id,
        prompt,
        wire_input: Value::Array(input.clone()),
        client_id: optional_string(params.get("clientUserMessageId"), "clientUserMessageId")?,
        model: optional_string(params.get("model"), "model")?,
    })
}

fn required_string(value: Option<&Value>, name: &str) -> std::result::Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{name} must be a string"))
}

pub(crate) fn optional_string(
    value: Option<&Value>,
    name: &str,
) -> std::result::Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{name} must be a string or null")),
    }
}

pub(crate) fn resolve_cwd(value: Option<&Value>) -> std::result::Result<String, String> {
    let cwd = match value {
        None | Some(Value::Null) => env::current_dir().map_err(|error| error.to_string())?,
        Some(Value::String(value)) => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                env::current_dir()
                    .map_err(|error| error.to_string())?
                    .join(path)
            }
        }
        Some(_) => return Err("cwd must be a string or null".to_string()),
    };
    Ok(cwd.to_string_lossy().into_owned())
}

pub(crate) fn thread_value(id: &str, cwd: &str, timestamp: i64) -> Value {
    json!({
        "id": id,
        "sessionId": id,
        "forkedFromId": null,
        "parentThreadId": null,
        "preview": "",
        "ephemeral": true,
        "modelProvider": "openai",
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "recencyAt": timestamp,
        "status": { "type": "idle" },
        "path": null,
        "cwd": cwd,
        "cliVersion": env!("CARGO_PKG_VERSION"),
        "source": "appServer",
        "threadSource": "user",
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": null,
        "turns": [],
    })
}

pub(crate) fn thread_start_result(thread: Value, model: &str, cwd: &str) -> Value {
    json!({
        "thread": thread,
        "model": model,
        "modelProvider": "openai",
        "serviceTier": null,
        "cwd": cwd,
        "instructionSources": [],
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "sandbox": { "type": "readOnly", "networkAccess": false },
        "reasoningEffort": null,
    })
}

pub(crate) fn turn_value(
    id: &str,
    status: &str,
    error: Value,
    started_at: Option<i64>,
    completed_at: Option<i64>,
    duration_ms: Option<i64>,
) -> Value {
    json!({
        "id": id,
        "items": [],
        "itemsView": "notLoaded",
        "status": status,
        "error": error,
        "startedAt": started_at,
        "completedAt": completed_at,
        "durationMs": duration_ms,
    })
}

pub(crate) fn agent_item_value(id: &str, text: &str) -> Value {
    json!({
        "type": "agentMessage",
        "id": id,
        "text": text,
        "phase": null,
        "memoryCitation": null,
    })
}

pub(crate) fn token_usage_value(usage: &TokenUsage, context_window: Option<i64>) -> Value {
    let breakdown = json!({
        "totalTokens": usage.total_tokens,
        "inputTokens": usage.input_tokens,
        "cachedInputTokens": usage.cached_input_tokens,
        "cacheWriteInputTokens": usage.cache_write_input_tokens,
        "outputTokens": usage.output_tokens,
        "reasoningOutputTokens": usage.reasoning_output_tokens,
    });
    json!({
        "total": breakdown,
        "last": breakdown,
        "modelContextWindow": context_window,
    })
}

pub(crate) fn notification(method: &str, params: Value) -> Value {
    json!({ "method": method, "params": params })
}

pub(crate) fn write_json_line(writer: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(crate) fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

pub(crate) fn duration_millis(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}
