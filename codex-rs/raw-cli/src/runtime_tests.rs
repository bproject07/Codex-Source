use std::time::Duration;

use anyhow::Result;
use anyhow::bail;
use axum::Json;
use axum::Router;
use axum::routing::post;
use codex_api::Provider;
use codex_api::ReqwestTransport;
use codex_api::ResponsesClient;
use codex_api::ResponsesOptions;
use codex_api::RetryConfig;
use codex_model_provider::unauthenticated_auth_provider;
use codex_protocol::protocol::TokenUsage;
use http::HeaderMap;
use http::header::CONTENT_TYPE;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use super::RawResponseStream;
use super::RawStreamEvent;
use super::build_minimal_request;

#[tokio::test]
async fn upstream_http_transport_preserves_raw_request_and_stream() -> Result<()> {
    let events = [
        json!({"type": "response.created", "response": {"id": "upstream-response"}}),
        json!({"type": "response.output_text.delta", "delta": "hello"}),
        json!({
            "type": "response.completed",
            "response": {
                "id": "upstream-response",
                "usage": {
                    "input_tokens": 7,
                    "input_tokens_details": {"cached_tokens": 2, "cache_write_tokens": 1},
                    "output_tokens": 5,
                    "output_tokens_details": {"reasoning_tokens": 3},
                    "total_tokens": 12,
                    "codex_rollout_budget_units": 9
                }
            }
        }),
    ];
    let body = events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    let (request_tx, mut request_rx) = mpsc::channel(/*buffer*/ 1);
    let router = Router::new().route(
        "/backend-api/codex/responses",
        post(move |Json(request): Json<Value>| {
            let body = body.clone();
            let request_tx = request_tx.clone();
            async move {
                request_tx.send(request).await.expect("capture request");
                ([(CONTENT_TYPE, "text/event-stream")], body)
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });
    let timeout = Duration::from_secs(5);
    let transport = ReqwestTransport::new(
        reqwest::Client::builder()
            .no_proxy()
            .timeout(timeout)
            .build()?,
    );
    let provider = Provider {
        name: "raw-test".to_string(),
        base_url: format!("http://{address}/backend-api/codex"),
        query_params: None,
        headers: HeaderMap::new(),
        retry: RetryConfig {
            max_attempts: 1,
            base_delay: Duration::ZERO,
            retry_429: false,
            retry_5xx: false,
            retry_transport: false,
        },
        stream_idle_timeout: timeout,
    };
    let inner = ResponsesClient::new(transport, provider, unauthenticated_auth_provider())
        .stream_request(
            build_minimal_request(
                "gpt-test".to_string(),
                "hello".to_string(),
                /*use_responses_lite*/ false,
            ),
            ResponsesOptions::default(),
        )
        .await?;
    let mut stream = RawResponseStream {
        inner,
        completed: false,
    };

    assert_eq!(
        request_rx.recv().await,
        Some(json!({
            "model": "gpt-test",
            "input": [{"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "hello"}
            ]}],
            "tool_choice": "none",
            "parallel_tool_calls": false,
            "reasoning": null,
            "store": false,
            "stream": true,
            "include": []
        }))
    );
    assert!(matches!(
        stream.next_event().await?,
        Some(RawStreamEvent::Created)
    ));
    let Some(RawStreamEvent::OutputTextDelta(text)) = stream.next_event().await? else {
        bail!("expected a text delta");
    };
    assert_eq!(text, "hello");
    let Some(RawStreamEvent::Completed { token_usage }) = stream.next_event().await? else {
        bail!("expected completion with token usage");
    };
    assert_eq!(
        token_usage,
        TokenUsage {
            input_tokens: 7,
            cached_input_tokens: 2,
            cache_write_input_tokens: 1,
            output_tokens: 5,
            reasoning_output_tokens: 3,
            total_tokens: 12,
            codex_rollout_budget_units: Some(9.into()),
        }
    );
    assert!(stream.next_event().await?.is_none());
    let _ = shutdown_tx.send(());
    server.await??;
    Ok(())
}
