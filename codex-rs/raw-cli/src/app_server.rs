use std::collections::HashMap;
use std::env;
use std::io;
use std::io::BufRead;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use serde_json::Value;
use serde_json::json;

use crate::app_server_wire::RpcMessage;
use crate::app_server_wire::agent_item_value;
use crate::app_server_wire::duration_millis;
use crate::app_server_wire::notification;
use crate::app_server_wire::optional_string;
use crate::app_server_wire::parse_message;
use crate::app_server_wire::parse_turn_input;
use crate::app_server_wire::resolve_cwd;
use crate::app_server_wire::thread_start_result;
use crate::app_server_wire::thread_value;
use crate::app_server_wire::token_usage_value;
use crate::app_server_wire::turn_value;
use crate::app_server_wire::unix_millis;
use crate::app_server_wire::unix_seconds;
use crate::app_server_wire::write_json_line;
use crate::runtime::RawRuntime;
use crate::runtime::ResolvedRawModel;

const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

pub(crate) async fn run(raw_home: &Path, default_model: Option<String>) -> Result<()> {
    let runtime = RawRuntime::new(raw_home).await?;
    let stdin = io::stdin();
    let mut server = AppServer::new(
        runtime,
        raw_home.to_string_lossy().into_owned(),
        default_model,
        io::stdout(),
    );

    for line in stdin.lock().lines() {
        let line = line?;
        if !line.trim().is_empty() {
            server.handle_line(&line).await?;
        }
    }
    Ok(())
}

struct AppServer<W: Write> {
    runtime: RawRuntime,
    raw_home: String,
    default_model: Option<String>,
    output: W,
    initialized: bool,
    threads: HashMap<String, RawThread>,
    next_id: u64,
}

#[derive(Clone)]
struct RawThread {
    model: ResolvedRawModel,
}

impl<W: Write> AppServer<W> {
    fn new(
        runtime: RawRuntime,
        raw_home: String,
        default_model: Option<String>,
        output: W,
    ) -> Self {
        Self {
            runtime,
            raw_home,
            default_model,
            output,
            initialized: false,
            threads: HashMap::new(),
            next_id: 1,
        }
    }

    async fn handle_line(&mut self, line: &str) -> Result<()> {
        let message = match parse_message(line) {
            Ok(message) => message,
            Err(message) => {
                return self.write_error(Value::Null, INVALID_REQUEST, message);
            }
        };

        if message.method == "initialize" {
            return self.handle_initialize(message);
        }
        if !self.initialized {
            return self.write_error_for(&message, INVALID_REQUEST, "Not initialized");
        }

        match message.method.as_str() {
            "initialized" => Ok(()),
            "thread/start" => self.handle_thread_start(message).await,
            "turn/start" => self.handle_turn_start(message).await,
            _ => self.write_error_for(&message, METHOD_NOT_FOUND, "Method not found"),
        }
    }

    fn handle_initialize(&mut self, message: RpcMessage) -> Result<()> {
        if self.initialized {
            return self.write_error_for(&message, INVALID_REQUEST, "Already initialized");
        }
        let Some(id) = message.id else {
            return self.write_error(Value::Null, INVALID_REQUEST, "initialize requires an id");
        };

        let result = json!({
            "userAgent": format!("codex-raw/{}", env!("CARGO_PKG_VERSION")),
            "codexHome": self.raw_home,
            "platformFamily": if cfg!(windows) { "windows" } else { "unix" },
            "platformOs": env::consts::OS,
        });
        self.write_response(id, result)?;
        self.initialized = true;
        Ok(())
    }

    async fn handle_thread_start(&mut self, message: RpcMessage) -> Result<()> {
        let Some(id) = message.id else {
            return self.write_error(Value::Null, INVALID_REQUEST, "thread/start requires an id");
        };
        let params = match message.params.as_object() {
            Some(params) => params,
            None => return self.write_error(id, INVALID_PARAMS, "params must be an object"),
        };
        let requested_model = match optional_string(params.get("model"), "model") {
            Ok(model) => model,
            Err(error) => return self.write_error(id, INVALID_PARAMS, error),
        };
        let cwd = match resolve_cwd(params.get("cwd")) {
            Ok(cwd) => cwd,
            Err(error) => return self.write_error(id, INVALID_PARAMS, error),
        };
        let requested_model = requested_model.as_deref().or(self.default_model.as_deref());
        let model = self.runtime.resolve_model(requested_model).await;
        let thread_id = self.allocate_id("raw-thread");
        let thread = thread_value(&thread_id, &cwd, unix_seconds());
        let result = thread_start_result(thread.clone(), &model.slug, &cwd);

        self.threads.insert(thread_id, RawThread { model });
        self.write_response(id, result)?;
        self.write_notification("thread/started", json!({ "thread": thread }))
    }

    async fn handle_turn_start(&mut self, message: RpcMessage) -> Result<()> {
        let Some(id) = message.id else {
            return self.write_error(Value::Null, INVALID_REQUEST, "turn/start requires an id");
        };
        let input = match parse_turn_input(&message.params) {
            Ok(input) => input,
            Err(error) => return self.write_error(id, INVALID_PARAMS, error),
        };
        let Some(thread) = self.threads.get(&input.thread_id).cloned() else {
            return self.write_error(id, INVALID_PARAMS, "unknown threadId");
        };
        let model = match input.model.as_deref() {
            Some(model) => self.runtime.resolve_model(Some(model)).await,
            None => thread.model,
        };

        let turn_id = self.allocate_id("raw-turn");
        let user_item_id = self.allocate_id("raw-item");
        let agent_item_id = self.allocate_id("raw-item");
        let started_at = unix_seconds();
        let started = Instant::now();
        let running_turn = turn_value(
            &turn_id,
            "inProgress",
            Value::Null,
            Some(started_at),
            None,
            None,
        );
        let user_item = json!({
            "type": "userMessage",
            "id": user_item_id,
            "clientId": input.client_id,
            "content": input.wire_input,
        });
        let empty_agent_item = agent_item_value(&agent_item_id, "");

        self.write_response(id, json!({ "turn": running_turn }))?;
        self.write_notification(
            "turn/started",
            json!({ "threadId": input.thread_id, "turn": running_turn }),
        )?;
        self.write_item_event("item/started", &input.thread_id, &turn_id, &user_item)?;
        self.write_item_event("item/completed", &input.thread_id, &turn_id, &user_item)?;
        self.write_item_event(
            "item/started",
            &input.thread_id,
            &turn_id,
            &empty_agent_item,
        )?;

        let runtime = self.runtime.clone();
        let thread_id = input.thread_id.clone();
        let output = &mut self.output;
        let mut full_text = String::new();
        let stream_result = runtime
            .stream_prompt(&model, input.prompt, |delta| {
                full_text.push_str(delta);
                write_json_line(
                    output,
                    &notification(
                        "item/agentMessage/delta",
                        json!({
                            "threadId": thread_id,
                            "turnId": turn_id,
                            "itemId": agent_item_id,
                            "delta": delta,
                        }),
                    ),
                )
            })
            .await;

        let completed_agent_item = agent_item_value(&agent_item_id, &full_text);
        self.write_item_event(
            "item/completed",
            &input.thread_id,
            &turn_id,
            &completed_agent_item,
        )?;
        let duration_ms = duration_millis(started);
        let completed_at = unix_seconds();

        match stream_result {
            Ok(usage) => {
                self.write_notification(
                    "thread/tokenUsage/updated",
                    json!({
                        "threadId": input.thread_id,
                        "turnId": turn_id,
                        "tokenUsage": token_usage_value(&usage, model.context_window),
                    }),
                )?;
                self.write_notification(
                    "turn/completed",
                    json!({
                        "threadId": input.thread_id,
                        "turn": turn_value(
                            &turn_id,
                            "completed",
                            Value::Null,
                            Some(started_at),
                            Some(completed_at),
                            Some(duration_ms),
                        ),
                    }),
                )
            }
            Err(error) => {
                let error_message = error.to_string();
                let turn_error = json!({
                    "message": error_message,
                    "codexErrorInfo": null,
                    "additionalDetails": null,
                });
                self.write_notification(
                    "error",
                    json!({
                        "error": turn_error,
                        "willRetry": false,
                        "threadId": input.thread_id,
                        "turnId": turn_id,
                    }),
                )?;
                self.write_notification(
                    "turn/completed",
                    json!({
                        "threadId": input.thread_id,
                        "turn": turn_value(
                            &turn_id,
                            "failed",
                            turn_error,
                            Some(started_at),
                            Some(completed_at),
                            Some(duration_ms),
                        ),
                    }),
                )
            }
        }
    }

    fn write_item_event(
        &mut self,
        method: &str,
        thread_id: &str,
        turn_id: &str,
        item: &Value,
    ) -> Result<()> {
        let timestamp_key = if method == "item/started" {
            "startedAtMs"
        } else {
            "completedAtMs"
        };
        let mut params = json!({
            "item": item,
            "threadId": thread_id,
            "turnId": turn_id,
        });
        params[timestamp_key] = json!(unix_millis());
        self.write_notification(method, params)
    }

    fn allocate_id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn write_response(&mut self, id: Value, result: Value) -> Result<()> {
        write_json_line(&mut self.output, &json!({ "id": id, "result": result }))
    }

    fn write_notification(&mut self, method: &str, params: Value) -> Result<()> {
        write_json_line(&mut self.output, &notification(method, params))
    }

    fn write_error_for(&mut self, message: &RpcMessage, code: i64, text: &str) -> Result<()> {
        match message.id.clone() {
            Some(id) => self.write_error(id, code, text),
            None => Ok(()),
        }
    }

    fn write_error(&mut self, id: Value, code: i64, message: impl Into<String>) -> Result<()> {
        write_json_line(
            &mut self.output,
            &json!({
                "id": id,
                "error": { "code": code, "message": message.into() },
            }),
        )
    }
}

#[cfg(test)]
#[path = "app_server_tests.rs"]
mod tests;
