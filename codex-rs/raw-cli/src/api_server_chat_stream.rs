use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use serde_json::Value;
use serde_json::json;

use crate::api_server_response::chat_usage;
use crate::api_server_stream::SseFrame;
use crate::api_server_stream::StreamMeta;

pub(crate) struct ChatStreamEncoder {
    meta: StreamMeta,
    include_usage: bool,
    next_tool_index: usize,
    saw_tool_call: bool,
}

impl ChatStreamEncoder {
    pub(crate) fn new(meta: StreamMeta, include_usage: bool) -> Self {
        Self {
            meta,
            include_usage,
            next_tool_index: 0,
            saw_tool_call: false,
        }
    }

    pub(crate) fn begin(&self) -> Vec<SseFrame> {
        vec![self.chunk(json!({ "role": "assistant" }), None)]
    }

    pub(crate) fn handle_text_delta(&self, delta: &str) -> Vec<SseFrame> {
        vec![self.chunk(json!({ "content": delta }), None)]
    }

    pub(crate) fn handle_item_done(&mut self, item: &ResponseItem) -> Vec<SseFrame> {
        let ResponseItem::FunctionCall {
            id,
            name,
            arguments,
            call_id,
            ..
        } = item
        else {
            return Vec::new();
        };

        self.saw_tool_call = true;
        let tool_call_id = if call_id.is_empty() {
            id.as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("call_{}", self.next_tool_index))
        } else {
            call_id.clone()
        };
        let tool_index = self.next_tool_index;
        self.next_tool_index += 1;
        vec![self.chunk(
            json!({
                "tool_calls": [{
                    "index": tool_index,
                    "id": tool_call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments,
                    },
                }],
            }),
            None,
        )]
    }

    pub(crate) fn completed(&self, usage: &TokenUsage) -> Vec<SseFrame> {
        let finish_reason = if self.saw_tool_call {
            "tool_calls"
        } else {
            "stop"
        };
        let mut frames = vec![self.chunk(json!({}), Some(finish_reason))];
        if self.include_usage {
            frames.push(SseFrame::json(
                None,
                &json!({
                    "id": self.meta.id,
                    "object": "chat.completion.chunk",
                    "created": self.meta.created,
                    "model": self.meta.model,
                    "choices": [],
                    "usage": chat_usage(usage),
                }),
            ));
        }
        frames.push(SseFrame::done());
        frames
    }

    pub(crate) fn failed(&self, message: &str) -> Vec<SseFrame> {
        vec![
            SseFrame::json(
                Some("error"),
                &json!({
                    "error": {
                        "message": message,
                        "type": "server_error",
                    },
                }),
            ),
            SseFrame::done(),
        ]
    }

    fn chunk(&self, delta: Value, finish_reason: Option<&str>) -> SseFrame {
        SseFrame::json(
            None,
            &json!({
                "id": self.meta.id,
                "object": "chat.completion.chunk",
                "created": self.meta.created,
                "model": self.meta.model,
                "choices": [{
                    "index": 0,
                    "delta": delta,
                    "logprobs": null,
                    "finish_reason": finish_reason,
                }],
            }),
        )
    }
}
