use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::response::IntoResponse;
use axum::response::Response;
use axum::response::sse::Event;
use axum::response::sse::KeepAlive;
use axum::response::sse::Sse;
use codex_protocol::models::ResponseItem;
use tokio::sync::OwnedSemaphorePermit;

use crate::api_server_chat_stream::ChatStreamEncoder;
use crate::api_server_stream::ResponsesStreamEncoder;
use crate::api_server_stream::SseFrame;
use crate::api_server_stream::StreamMeta;
use crate::api_server_wire::ParsedChatRequest;
use crate::api_server_wire::ParsedRequest;
use crate::runtime::RawResponseStream;
use crate::runtime::RawStreamEvent;

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

pub(crate) fn responses_sse(
    mut stream: RawResponseStream,
    request: ParsedRequest,
    model: String,
    response_id: String,
    created: i64,
    permit: OwnedSemaphorePermit,
) -> Response {
    let response_stream = async_stream::stream! {
        let _permit = permit;
        let mut encoder = ResponsesStreamEncoder::new(
            StreamMeta::new(response_id.clone(), model, created),
            &request,
        );
        let mut indexes = StreamIndexes::default();
        let mut next_generated_id = 1_u64;
        let mut id_factory = || {
            let id = format!("item_{response_id}_{next_generated_id}");
            next_generated_id += 1;
            id
        };
        for frame in encoder.begin() {
            yield Ok::<Event, Infallible>(sse_event(frame));
        }

        loop {
            match stream.next_event().await {
                Ok(Some(RawStreamEvent::Created)) => {}
                Ok(Some(RawStreamEvent::OutputItemAdded(item))) => {
                    let index = indexes.added(&item);
                    for frame in encoder.handle_item_added(index, &item, &mut id_factory) {
                        yield Ok(sse_event(frame));
                    }
                }
                Ok(Some(RawStreamEvent::OutputTextDelta(delta))) => {
                    let index = indexes.current();
                    for frame in encoder.handle_text_delta(index, 0, &delta, &mut id_factory) {
                        yield Ok(sse_event(frame));
                    }
                }
                Ok(Some(RawStreamEvent::ToolCallInputDelta { item_id, call_id, delta })) => {
                    let index = indexes.for_id(&item_id);
                    for frame in encoder.handle_tool_delta(
                        index,
                        Some(&item_id),
                        call_id.as_deref(),
                        &delta,
                        &mut id_factory,
                    ) {
                        yield Ok(sse_event(frame));
                    }
                }
                Ok(Some(RawStreamEvent::OutputItemDone(item))) => {
                    let index = indexes.done(&item);
                    for frame in encoder.handle_item_done(index, &item, &mut id_factory) {
                        yield Ok(sse_event(frame));
                    }
                }
                Ok(Some(RawStreamEvent::Completed { token_usage, .. })) => {
                    for frame in encoder.completed(&token_usage) {
                        yield Ok(sse_event(frame));
                    }
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    for frame in encoder.failed("The upstream response stream failed") {
                        yield Ok(sse_event(frame));
                    }
                    break;
                }
            }
        }
    };

    Sse::new(response_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(KEEP_ALIVE_INTERVAL)
                .text("keep-alive"),
        )
        .into_response()
}

pub(crate) fn chat_sse(
    mut stream: RawResponseStream,
    request: ParsedChatRequest,
    model: String,
    response_id: String,
    created: i64,
    permit: OwnedSemaphorePermit,
) -> Response {
    let response_stream = async_stream::stream! {
        let _permit = permit;
        let mut encoder = ChatStreamEncoder::new(
            StreamMeta::new(response_id, model, created),
            request.include_usage,
        );
        for frame in encoder.begin() {
            yield Ok::<Event, Infallible>(sse_event(frame));
        }

        loop {
            match stream.next_event().await {
                Ok(Some(RawStreamEvent::OutputTextDelta(delta))) => {
                    for frame in encoder.handle_text_delta(&delta) {
                        yield Ok(sse_event(frame));
                    }
                }
                Ok(Some(RawStreamEvent::OutputItemDone(item))) => {
                    for frame in encoder.handle_item_done(&item) {
                        yield Ok(sse_event(frame));
                    }
                }
                Ok(Some(RawStreamEvent::Completed { token_usage, .. })) => {
                    for frame in encoder.completed(&token_usage) {
                        yield Ok(sse_event(frame));
                    }
                    break;
                }
                Ok(Some(RawStreamEvent::Created))
                | Ok(Some(RawStreamEvent::OutputItemAdded(_)))
                | Ok(Some(RawStreamEvent::ToolCallInputDelta { .. })) => {}
                Ok(None) => break,
                Err(_) => {
                    for frame in encoder.failed("The upstream response stream failed") {
                        yield Ok(sse_event(frame));
                    }
                    break;
                }
            }
        }
    };

    Sse::new(response_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(KEEP_ALIVE_INTERVAL)
                .text("keep-alive"),
        )
        .into_response()
}

fn sse_event(frame: SseFrame) -> Event {
    let event = Event::default().data(frame.data);
    match frame.event {
        Some(name) => event.event(name),
        None => event,
    }
}

fn response_item_id(item: &ResponseItem) -> Option<String> {
    item.id().map(ToString::to_string)
}

#[derive(Default)]
struct StreamIndexes {
    by_id: HashMap<String, usize>,
    current: Option<usize>,
    next: usize,
}

impl StreamIndexes {
    fn added(&mut self, item: &ResponseItem) -> usize {
        let index = self.next;
        self.next += 1;
        self.current = Some(index);
        if let Some(id) = response_item_id(item) {
            self.by_id.insert(id, index);
        }
        index
    }

    fn current(&mut self) -> usize {
        match self.current {
            Some(index) => index,
            None => self.allocate_current(),
        }
    }

    fn for_id(&mut self, id: &str) -> usize {
        self.by_id.get(id).copied().unwrap_or_else(|| {
            let index = self.allocate_current();
            self.by_id.insert(id.to_string(), index);
            index
        })
    }

    fn done(&mut self, item: &ResponseItem) -> usize {
        if let Some(id) = response_item_id(item)
            && let Some(index) = self.by_id.get(&id).copied()
        {
            self.current = Some(index);
            return index;
        }
        self.current()
    }

    fn allocate_current(&mut self) -> usize {
        let index = self.next;
        self.next += 1;
        self.current = Some(index);
        index
    }
}
