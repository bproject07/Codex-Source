use std::collections::BTreeMap;
use std::collections::BTreeSet;

use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use serde_json::Value;
use serde_json::json;

use crate::api_server_response::response_item_started_value;
use crate::api_server_response::response_item_text;
use crate::api_server_response::response_item_value;
use crate::api_server_response::responses_usage;
use crate::api_server_wire::ParsedRequest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SseFrame {
    pub(crate) event: Option<String>,
    pub(crate) data: String,
}

impl SseFrame {
    pub(crate) fn json(event: Option<&str>, value: &Value) -> Self {
        Self {
            event: event.map(str::to_string),
            data: value.to_string(),
        }
    }

    pub(crate) fn done() -> Self {
        Self {
            event: None,
            data: "[DONE]".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StreamMeta {
    pub(crate) id: String,
    pub(crate) model: String,
    pub(crate) created: i64,
}

impl StreamMeta {
    pub(crate) fn new(id: impl Into<String>, model: impl Into<String>, created: i64) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            created,
        }
    }
}

pub(crate) struct ResponsesStreamEncoder {
    meta: StreamMeta,
    settings: ResponseStreamSettings,
    sequence_number: u64,
    item_ids: BTreeMap<usize, String>,
    added_items: BTreeSet<usize>,
    added_text_parts: BTreeSet<usize>,
    streamed_text: BTreeMap<usize, String>,
    completed_items: BTreeMap<usize, Value>,
}

struct ResponseStreamSettings {
    instructions: Option<String>,
    parallel_tool_calls: bool,
    reasoning_effort: Option<String>,
    service_tier: Option<String>,
    tool_choice: String,
    tools: Vec<Value>,
}

impl ResponsesStreamEncoder {
    pub(crate) fn new(meta: StreamMeta, request: &ParsedRequest) -> Self {
        Self {
            meta,
            settings: ResponseStreamSettings {
                instructions: request.instructions.clone(),
                parallel_tool_calls: request.parallel_tool_calls,
                reasoning_effort: request.reasoning_effort.as_ref().map(ToString::to_string),
                service_tier: request.service_tier.clone(),
                tool_choice: request.tool_choice.clone(),
                tools: request.tools.clone().unwrap_or_default(),
            },
            sequence_number: 0,
            item_ids: BTreeMap::new(),
            added_items: BTreeSet::new(),
            added_text_parts: BTreeSet::new(),
            streamed_text: BTreeMap::new(),
            completed_items: BTreeMap::new(),
        }
    }

    pub(crate) fn begin(&mut self) -> Vec<SseFrame> {
        let created = self.response_lifecycle_frame("response.created", "in_progress", None, None);
        let in_progress =
            self.response_lifecycle_frame("response.in_progress", "in_progress", None, None);
        vec![created, in_progress]
    }

    pub(crate) fn handle_item_added(
        &mut self,
        output_index: usize,
        item: &ResponseItem,
        next_item_id: &mut impl FnMut() -> String,
    ) -> Vec<SseFrame> {
        let item_id = self.resolve_item_id(output_index, item, next_item_id);
        if !self.added_items.insert(output_index) {
            return Vec::new();
        }
        let item = response_item_started_value(item, &item_id);
        vec![self.event_frame(
            "response.output_item.added",
            json!({
                "output_index": output_index,
                "item": item,
            }),
        )]
    }

    pub(crate) fn handle_text_delta(
        &mut self,
        output_index: usize,
        content_index: usize,
        delta: &str,
        next_item_id: &mut impl FnMut() -> String,
    ) -> Vec<SseFrame> {
        let item_id = self.resolve_item_id_without_item(output_index, next_item_id);
        let mut frames = self.ensure_text_part(output_index, &item_id);
        self.streamed_text
            .entry(output_index)
            .or_default()
            .push_str(delta);
        frames.push(self.event_frame(
            "response.output_text.delta",
            json!({
                "item_id": item_id,
                "output_index": output_index,
                "content_index": content_index,
                "delta": delta,
                "logprobs": [],
            }),
        ));
        frames
    }

    pub(crate) fn handle_tool_delta(
        &mut self,
        output_index: usize,
        item_id: Option<&str>,
        call_id: Option<&str>,
        delta: &str,
        next_item_id: &mut impl FnMut() -> String,
    ) -> Vec<SseFrame> {
        let item_id = match item_id {
            Some(item_id) => {
                self.item_ids.insert(output_index, item_id.to_string());
                item_id.to_string()
            }
            None => self.resolve_item_id_without_item(output_index, next_item_id),
        };
        let mut payload = json!({
            "item_id": item_id,
            "output_index": output_index,
            "delta": delta,
        });
        if let Some(call_id) = call_id {
            payload["call_id"] = json!(call_id);
        }
        vec![self.event_frame("response.custom_tool_call_input.delta", payload)]
    }

    pub(crate) fn handle_item_done(
        &mut self,
        output_index: usize,
        item: &ResponseItem,
        next_item_id: &mut impl FnMut() -> String,
    ) -> Vec<SseFrame> {
        let item_id = self.resolve_item_id(output_index, item, next_item_id);
        let item_value = response_item_value(item, &item_id, "completed");
        let mut frames = if self.added_items.insert(output_index) {
            vec![self.output_item_added_frame(output_index, item, &item_id)]
        } else {
            Vec::new()
        };

        if let ResponseItem::FunctionCall {
            name, arguments, ..
        } = item
        {
            frames.push(self.event_frame(
                "response.function_call_arguments.delta",
                json!({
                    "item_id": item_id,
                    "output_index": output_index,
                    "delta": arguments,
                }),
            ));
            frames.push(self.event_frame(
                "response.function_call_arguments.done",
                json!({
                    "item_id": item_id,
                    "output_index": output_index,
                    "name": name,
                    "arguments": arguments,
                }),
            ));
        } else if let Some(final_text) = response_item_text(item) {
            frames.extend(self.ensure_text_part(output_index, &item_id));
            let streamed_text = self.streamed_text.entry(output_index).or_default().clone();
            if let Some(remainder) = final_text.strip_prefix(&streamed_text)
                && !remainder.is_empty()
            {
                self.streamed_text
                    .entry(output_index)
                    .or_default()
                    .push_str(remainder);
                frames.push(self.event_frame(
                    "response.output_text.delta",
                    json!({
                        "item_id": item_id,
                        "output_index": output_index,
                        "content_index": 0,
                        "delta": remainder,
                        "logprobs": [],
                    }),
                ));
            }
            frames.push(self.event_frame(
                "response.output_text.done",
                json!({
                    "item_id": item_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "text": final_text,
                    "logprobs": [],
                }),
            ));
            frames.push(self.event_frame(
                "response.content_part.done",
                json!({
                    "item_id": item_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "part": output_text_part(&final_text),
                }),
            ));
        }

        self.completed_items
            .insert(output_index, item_value.clone());
        frames.push(self.event_frame(
            "response.output_item.done",
            json!({
                "output_index": output_index,
                "item": item_value,
            }),
        ));
        frames
    }

    fn ensure_text_part(&mut self, output_index: usize, item_id: &str) -> Vec<SseFrame> {
        let mut frames = Vec::new();
        if self.added_items.insert(output_index) {
            frames.push(self.event_frame(
                "response.output_item.added",
                json!({
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": [],
                    },
                }),
            ));
        }
        if self.added_text_parts.insert(output_index) {
            frames.push(self.event_frame(
                "response.content_part.added",
                json!({
                    "item_id": item_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "part": output_text_part(""),
                }),
            ));
        }
        frames
    }

    fn output_item_added_frame(
        &mut self,
        output_index: usize,
        item: &ResponseItem,
        item_id: &str,
    ) -> SseFrame {
        self.event_frame(
            "response.output_item.added",
            json!({
                "output_index": output_index,
                "item": response_item_started_value(item, item_id),
            }),
        )
    }

    pub(crate) fn completed(&mut self, usage: &TokenUsage) -> Vec<SseFrame> {
        let output = self.completed_items.values().cloned().collect::<Vec<_>>();
        vec![self.response_lifecycle_frame(
            "response.completed",
            "completed",
            Some(output),
            Some(usage),
        )]
    }

    pub(crate) fn failed(&mut self, message: &str) -> Vec<SseFrame> {
        let output = self.completed_items.values().cloned().collect::<Vec<_>>();
        let response = response_value(
            &self.meta,
            &self.settings,
            "failed",
            output,
            None,
            Some(json!({
                "code": "server_error",
                "message": message,
            })),
        );
        vec![self.event_frame("response.failed", json!({ "response": response }))]
    }

    fn response_lifecycle_frame(
        &mut self,
        event: &str,
        status: &str,
        output: Option<Vec<Value>>,
        usage: Option<&TokenUsage>,
    ) -> SseFrame {
        let response = response_value(
            &self.meta,
            &self.settings,
            status,
            output.unwrap_or_default(),
            usage,
            None,
        );
        self.event_frame(event, json!({ "response": response }))
    }

    fn event_frame(&mut self, event: &str, payload: Value) -> SseFrame {
        let mut payload = payload.as_object().cloned().unwrap_or_default();
        payload.insert("type".to_string(), json!(event));
        payload.insert("sequence_number".to_string(), json!(self.sequence_number));
        self.sequence_number += 1;
        SseFrame::json(Some(event), &Value::Object(payload))
    }

    fn resolve_item_id(
        &mut self,
        output_index: usize,
        item: &ResponseItem,
        next_item_id: &mut impl FnMut() -> String,
    ) -> String {
        if let Some(item_id) = self.item_ids.get(&output_index) {
            return item_id.clone();
        }
        if let Some(item_id) = item.id() {
            let item_id = item_id.to_string();
            self.item_ids.insert(output_index, item_id.clone());
            return item_id;
        }
        self.resolve_item_id_without_item(output_index, next_item_id)
    }

    fn resolve_item_id_without_item(
        &mut self,
        output_index: usize,
        next_item_id: &mut impl FnMut() -> String,
    ) -> String {
        self.item_ids
            .entry(output_index)
            .or_insert_with(next_item_id)
            .clone()
    }
}

fn output_text_part(text: &str) -> Value {
    json!({
        "type": "output_text",
        "text": text,
        "annotations": [],
        "logprobs": [],
    })
}

fn response_value(
    meta: &StreamMeta,
    settings: &ResponseStreamSettings,
    status: &str,
    output: Vec<Value>,
    usage: Option<&TokenUsage>,
    error: Option<Value>,
) -> Value {
    json!({
        "id": meta.id,
        "object": "response",
        "created_at": meta.created,
        "completed_at": (status == "completed").then_some(meta.created),
        "status": status,
        "background": false,
        "error": error,
        "incomplete_details": null,
        "instructions": settings.instructions,
        "max_output_tokens": null,
        "model": meta.model,
        "output": output,
        "parallel_tool_calls": settings.parallel_tool_calls,
        "previous_response_id": null,
        "reasoning": {
            "effort": settings.reasoning_effort,
            "summary": null,
        },
        "service_tier": settings.service_tier,
        "store": false,
        "temperature": null,
        "text": {"format": {"type": "text"}},
        "tool_choice": settings.tool_choice,
        "tools": settings.tools,
        "top_p": null,
        "truncation": "disabled",
        "usage": usage.map(responses_usage),
        "user": null,
        "metadata": {},
    })
}

#[cfg(test)]
#[path = "api_server_stream_tests.rs"]
mod tests;
