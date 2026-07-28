use codex_protocol::protocol::TokenUsage;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::agent_item_value;
use super::notification;
use super::parse_message;
use super::parse_turn_input;
use super::thread_start_result;
use super::thread_value;
use super::token_usage_value;
use super::turn_value;
use super::write_json_line;

#[test]
fn parses_numeric_and_string_request_ids() {
    let numeric =
        parse_message(r#"{"id":7,"method":"initialize"}"#).expect("parse numeric request id");
    let string =
        parse_message(r#"{"id":"seven","method":"initialize"}"#).expect("parse string request id");

    assert_eq!(numeric.id, Some(json!(7)));
    assert_eq!(string.id, Some(json!("seven")));
    assert_eq!(numeric.params, json!({}));
}

#[test]
fn rejects_non_scalar_request_id() {
    let error = parse_message(r#"{"id":{},"method":"initialize"}"#)
        .expect_err("object request id must fail");

    assert!(error.contains("string or number"));
}

#[test]
fn parses_exactly_one_text_input() {
    let input = parse_turn_input(&json!({
        "threadId": "raw-thread-1",
        "clientUserMessageId": "client-1",
        "model": "gpt-test",
        "input": [{ "type": "text", "text": "hello" }],
    }))
    .expect("parse Raw turn input");

    assert_eq!(input.thread_id, "raw-thread-1");
    assert_eq!(input.prompt, "hello");
    assert_eq!(input.client_id.as_deref(), Some("client-1"));
    assert_eq!(input.model.as_deref(), Some("gpt-test"));
    assert_eq!(
        input.wire_input,
        json!([{ "type": "text", "text": "hello" }])
    );
}

#[test]
fn rejects_history_and_non_text_inputs() {
    let multiple = parse_turn_input(&json!({
        "threadId": "raw-thread-1",
        "input": [
            { "type": "text", "text": "first" },
            { "type": "text", "text": "second" }
        ],
    }))
    .expect_err("multiple inputs must fail");
    let image = parse_turn_input(&json!({
        "threadId": "raw-thread-1",
        "input": [{ "type": "image", "url": "https://example.test/image.png" }],
    }))
    .expect_err("image input must fail");

    assert!(multiple.contains("exactly one"));
    assert!(image.contains("only text"));
}

#[test]
fn thread_and_turn_values_match_the_stable_v2_shape() {
    let thread = thread_value("raw-thread-1", "C:\\work", 123);
    let start_result = thread_start_result(thread.clone(), "gpt-test", "C:\\work");
    let turn = turn_value(
        "raw-turn-2",
        "inProgress",
        serde_json::Value::Null,
        Some(123),
        None,
        None,
    );

    assert_eq!(thread["id"], "raw-thread-1");
    assert_eq!(thread["sessionId"], "raw-thread-1");
    assert_eq!(thread["ephemeral"], true);
    assert_eq!(thread["status"], json!({ "type": "idle" }));
    assert_eq!(thread["source"], "appServer");
    assert_eq!(thread["turns"], json!([]));
    assert_eq!(
        start_result["sandbox"],
        json!({ "type": "readOnly", "networkAccess": false })
    );
    assert_eq!(turn["status"], "inProgress");
    assert_eq!(turn["itemsView"], "notLoaded");
}

#[test]
fn agent_delta_and_completed_item_share_an_id() {
    let delta = notification(
        "item/agentMessage/delta",
        json!({
            "threadId": "raw-thread-1",
            "turnId": "raw-turn-2",
            "itemId": "raw-item-3",
            "delta": "Hel",
        }),
    );
    let completed = agent_item_value("raw-item-3", "Hello");

    assert_eq!(delta["params"]["itemId"], completed["id"]);
    assert_eq!(completed["text"], "Hello");
}

#[test]
fn maps_usage_to_total_and_last_without_adding_tokens() {
    let usage = TokenUsage {
        input_tokens: 7,
        cached_input_tokens: 1,
        cache_write_input_tokens: 0,
        output_tokens: 13,
        reasoning_output_tokens: 2,
        total_tokens: 20,
    };
    let value = token_usage_value(&usage, Some(128_000));

    assert_eq!(value["total"], value["last"]);
    assert_eq!(value["last"]["inputTokens"], 7);
    assert_eq!(value["last"]["outputTokens"], 13);
    assert_eq!(value["modelContextWindow"], 128_000);
}

#[test]
fn jsonl_writer_uses_one_flushed_record_without_jsonrpc_field() {
    let mut output = Vec::new();
    let value = notification("initialized", json!({}));

    write_json_line(&mut output, &value).expect("write JSONL record");

    assert_eq!(
        String::from_utf8(output).expect("valid UTF-8"),
        "{\"method\":\"initialized\",\"params\":{}}\n"
    );
    assert_eq!(value.get("jsonrpc"), None);
}
