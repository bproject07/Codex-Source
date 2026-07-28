use codex_protocol::ResponseItemId;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::ResponsesStreamEncoder;
use super::SseFrame;
use super::StreamMeta;
use crate::api_server_chat_stream::ChatStreamEncoder;
use crate::api_server_wire::ParsedRequest;
use crate::api_server_wire::parse_responses_request;

fn meta() -> StreamMeta {
    StreamMeta::new("resp_test", "gpt-test", /*created*/ 123)
}

fn usage() -> TokenUsage {
    TokenUsage {
        input_tokens: 7,
        cached_input_tokens: 2,
        cache_write_input_tokens: 1,
        output_tokens: 13,
        reasoning_output_tokens: 3,
        total_tokens: 20,
    }
}

fn request() -> ParsedRequest {
    parse_responses_request(json!({ "input": "hello" })).expect("request")
}

fn json_data(frame: &SseFrame) -> Value {
    serde_json::from_str(&frame.data).expect("frame data should be JSON")
}

fn message(id: Option<&str>, text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: id.map(|id| ResponseItemId::from_server(id.to_string())),
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn function_call(id: Option<&str>) -> ResponseItem {
    ResponseItem::FunctionCall {
        id: id.map(|id| ResponseItemId::from_server(id.to_string())),
        name: "weather".to_string(),
        namespace: None,
        arguments: r#"{"city":"Sofia"}"#.to_string(),
        call_id: "call_weather".to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn responses_begin_has_stable_metadata_and_monotonic_sequence() {
    let mut encoder = ResponsesStreamEncoder::new(meta(), &request());

    let frames = encoder.begin();

    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.event.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("response.created"), Some("response.in_progress")]
    );
    let values = frames.iter().map(json_data).collect::<Vec<_>>();
    assert_eq!(values[0]["sequence_number"], 0);
    assert_eq!(values[1]["sequence_number"], 1);
    assert_eq!(values[0]["response"]["id"], "resp_test");
    assert_eq!(values[0]["response"]["created_at"], 123);
    assert_eq!(values[0]["response"]["model"], "gpt-test");
}

#[test]
fn responses_preserves_explicit_id_and_reuses_generated_id() {
    let mut encoder = ResponsesStreamEncoder::new(meta(), &request());
    let mut generated = 0;
    let mut next_id = || {
        generated += 1;
        format!("msg_generated_{generated}")
    };
    let _ = encoder.begin();

    let explicit = encoder.handle_item_added(
        /*output_index*/ 0,
        &message(Some("msg_explicit"), ""),
        &mut next_id,
    );
    let added =
        encoder.handle_item_added(/*output_index*/ 1, &message(None, ""), &mut next_id);
    let delta = encoder.handle_text_delta(
        /*output_index*/ 1,
        /*content_index*/ 0,
        "Hi",
        &mut next_id,
    );
    let done =
        encoder.handle_item_done(/*output_index*/ 1, &message(None, "Hi"), &mut next_id);

    assert_eq!(json_data(&explicit[0])["item"]["id"], "msg_explicit");
    assert_eq!(json_data(&added[0])["item"]["id"], "msg_generated_1");
    assert_eq!(json_data(&added[0])["item"]["content"], json!([]));
    assert_eq!(json_data(&delta[0])["item_id"], "msg_generated_1");
    assert_eq!(
        delta
            .iter()
            .map(|frame| frame.event.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("response.content_part.added"),
            Some("response.output_text.delta"),
        ]
    );
    assert_eq!(
        json_data(done.last().expect("done frame"))["item"]["id"],
        "msg_generated_1"
    );
    assert_eq!(generated, 1);
}

#[test]
fn responses_text_stream_has_the_sdk_compatible_content_lifecycle() {
    let mut encoder = ResponsesStreamEncoder::new(meta(), &request());
    let mut next_id = || "msg_stream".to_string();
    let _ = encoder.begin();
    let mut frames = encoder.handle_item_added(0, &message(None, ""), &mut next_id);
    frames.extend(encoder.handle_text_delta(0, 0, "Hel", &mut next_id));
    frames.extend(encoder.handle_text_delta(0, 0, "lo", &mut next_id));
    frames.extend(encoder.handle_item_done(0, &message(None, "Hello"), &mut next_id));

    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.event.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("response.output_item.added"),
            Some("response.content_part.added"),
            Some("response.output_text.delta"),
            Some("response.output_text.delta"),
            Some("response.output_text.done"),
            Some("response.content_part.done"),
            Some("response.output_item.done"),
        ]
    );
    assert_eq!(json_data(&frames[0])["item"]["content"], json!([]));
    assert_eq!(json_data(&frames[1])["part"]["text"], "");
    assert_eq!(json_data(&frames[4])["text"], "Hello");
    assert_eq!(json_data(&frames[5])["part"]["text"], "Hello");
}

#[test]
fn responses_synthesizes_function_argument_events_before_done() {
    let mut encoder = ResponsesStreamEncoder::new(meta(), &request());
    let mut next_id = || "fc_generated".to_string();
    let _ = encoder.begin();

    let frames = encoder.handle_item_done(
        /*output_index*/ 0,
        &function_call(Some("fc_explicit")),
        &mut next_id,
    );

    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.event.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("response.output_item.added"),
            Some("response.function_call_arguments.delta"),
            Some("response.function_call_arguments.done"),
            Some("response.output_item.done"),
        ]
    );
    let values = frames.iter().map(json_data).collect::<Vec<_>>();
    assert_eq!(values[0]["item"]["arguments"], "");
    assert_eq!(values[1]["item_id"], "fc_explicit");
    assert_eq!(values[1]["delta"], json!(r#"{"city":"Sofia"}"#));
    assert_eq!(values[2]["arguments"], json!(r#"{"city":"Sofia"}"#));
    assert_eq!(values[3]["item"]["id"], "fc_explicit");
    assert_eq!(values[0]["sequence_number"], 2);
    assert_eq!(values[3]["sequence_number"], 5);
}

#[test]
fn responses_completed_contains_items_and_exact_usage_without_done_sentinel() {
    let mut encoder = ResponsesStreamEncoder::new(meta(), &request());
    let mut next_id = || "msg_final".to_string();
    let _ = encoder.begin();
    let _ = encoder.handle_item_done(
        /*output_index*/ 0,
        &message(None, "Hello"),
        &mut next_id,
    );

    let frames = encoder.completed(&usage());

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].event.as_deref(), Some("response.completed"));
    assert_ne!(frames[0].data, "[DONE]");
    let value = json_data(&frames[0]);
    assert_eq!(value["response"]["status"], "completed");
    assert_eq!(value["response"]["output"][0]["id"], "msg_final");
    assert_eq!(value["response"]["usage"]["input_tokens"], 7);
    assert_eq!(
        value["response"]["usage"]["input_tokens_details"]["cached_tokens"],
        2
    );
    assert_eq!(value["response"]["usage"]["output_tokens"], 13);
}

#[test]
fn responses_failed_is_terminal_without_done_sentinel() {
    let mut encoder = ResponsesStreamEncoder::new(meta(), &request());
    let _ = encoder.begin();

    let frames = encoder.failed("backend unavailable");

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].event.as_deref(), Some("response.failed"));
    assert_ne!(frames[0].data, "[DONE]");
    let value = json_data(&frames[0]);
    assert_eq!(value["response"]["status"], "failed");
    assert_eq!(value["response"]["error"]["message"], "backend unavailable");
    assert_eq!(value["sequence_number"], 2);
}

#[test]
fn chat_stream_emits_role_text_tool_finish_usage_and_done() {
    let mut encoder = ChatStreamEncoder::new(meta(), true);
    let mut frames = encoder.begin();
    frames.extend(encoder.handle_text_delta("Checking"));
    frames.extend(encoder.handle_item_done(&function_call(Some("fc_item"))));
    frames.extend(encoder.completed(&usage()));

    assert_eq!(
        json_data(&frames[0])["choices"][0]["delta"]["role"],
        "assistant"
    );
    assert_eq!(
        json_data(&frames[1])["choices"][0]["delta"]["content"],
        "Checking"
    );
    let tool = json_data(&frames[2]);
    assert_eq!(
        tool["choices"][0]["delta"]["tool_calls"][0]["id"],
        "call_weather"
    );
    assert_eq!(
        tool["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
        json!(r#"{"city":"Sofia"}"#)
    );
    assert_eq!(
        json_data(&frames[3])["choices"][0]["finish_reason"],
        "tool_calls"
    );
    assert_eq!(json_data(&frames[4])["choices"], json!([]));
    assert_eq!(json_data(&frames[4])["usage"]["prompt_tokens"], 7);
    assert_eq!(
        frames[5],
        SseFrame {
            event: None,
            data: "[DONE]".to_string()
        }
    );
}

#[test]
fn chat_without_tools_finishes_with_stop_and_can_omit_usage() {
    let encoder = ChatStreamEncoder::new(meta(), false);

    let frames = encoder.completed(&usage());

    assert_eq!(frames.len(), 2);
    assert_eq!(json_data(&frames[0])["choices"][0]["finish_reason"], "stop");
    assert_eq!(frames[1].data, "[DONE]");
}

#[test]
fn chat_failure_emits_error_then_done() {
    let encoder = ChatStreamEncoder::new(meta(), false);

    let frames = encoder.failed("bad gateway");

    assert_eq!(frames[0].event.as_deref(), Some("error"));
    assert_eq!(json_data(&frames[0])["error"]["message"], "bad gateway");
    assert_eq!(frames[1].data, "[DONE]");
}
