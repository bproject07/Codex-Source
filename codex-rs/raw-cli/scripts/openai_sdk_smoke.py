#!/usr/bin/env python3
"""Live compatibility smoke test for a running Codex Raw API server."""

import argparse
import os

from openai import OpenAI


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="http://127.0.0.1:8080/v1")
    parser.add_argument("--model", default="gpt-5.4-mini")
    parser.add_argument("--include-tools", action="store_true")
    args = parser.parse_args()

    api_key = os.environ.get("CODEX_RAW_API_TOKEN") or "raw-local"
    client = OpenAI(api_key=api_key, base_url=args.base_url, timeout=120.0)

    response = client.responses.create(model=args.model, input="hello")
    print(
        "responses_nonstream",
        response.status,
        repr(response.output_text),
        response.usage.input_tokens,
    )

    with client.responses.stream(model=args.model, input="hello") as stream:
        streamed_text = "".join(
            event.delta
            for event in stream
            if event.type == "response.output_text.delta"
        )
        final_response = stream.get_final_response()
    print(
        "responses_stream",
        final_response.status,
        repr(streamed_text),
        repr(final_response.output_text),
        final_response.usage.input_tokens,
    )

    if args.include_tools:
        tool = {
            "type": "function",
            "name": "get_weather",
            "description": "Return weather for a city",
            "parameters": {
                "type": "object",
                "properties": {"city": {"type": "string"}},
                "required": ["city"],
                "additionalProperties": False,
            },
            "strict": True,
        }
        with client.responses.stream(
            model=args.model,
            instructions="You must call get_weather exactly once.",
            input="What is the weather in Sofia?",
            tools=[tool],
            tool_choice="required",
            parallel_tool_calls=False,
        ) as tool_stream:
            argument_deltas = "".join(
                event.delta
                for event in tool_stream
                if event.type == "response.function_call_arguments.delta"
            )
            tool_response = tool_stream.get_final_response()
        call = next(item for item in tool_response.output if item.type == "function_call")
        replay = [
            {"role": "user", "content": "What is the weather in Sofia?"},
            *(item.model_dump(exclude_none=True) for item in tool_response.output),
            {
                "type": "function_call_output",
                "call_id": call.call_id,
                "output": '{"temperature":24,"unit":"C"}',
            },
        ]
        tool_final = client.responses.create(
            model=args.model,
            instructions=(
                "Use the supplied result. Reply with one short sentence and do not "
                "call another tool."
            ),
            input=replay,
            tools=[tool],
            tool_choice="none",
            parallel_tool_calls=False,
        )
        if argument_deltas != call.arguments:
            raise RuntimeError("Streamed function arguments differ from the final call")
        print(
            "responses_tool_roundtrip",
            call.name,
            call.arguments,
            repr(tool_final.output_text),
        )

    completion = client.chat.completions.create(
        model=args.model,
        messages=[{"role": "user", "content": "hello"}],
    )
    print(
        "chat_nonstream",
        completion.choices[0].finish_reason,
        repr(completion.choices[0].message.content),
        completion.usage.prompt_tokens,
    )

    chunks = client.chat.completions.create(
        model=args.model,
        messages=[{"role": "user", "content": "hello"}],
        stream=True,
        stream_options={"include_usage": True},
    )
    chat_text = ""
    finish_reason = None
    usage = None
    for chunk in chunks:
        if chunk.choices:
            chat_text += chunk.choices[0].delta.content or ""
            finish_reason = chunk.choices[0].finish_reason or finish_reason
        if chunk.usage is not None:
            usage = chunk.usage
    if usage is None:
        raise RuntimeError("Chat stream ended without the requested usage chunk")
    print("chat_stream", finish_reason, repr(chat_text), usage.prompt_tokens)


if __name__ == "__main__":
    main()
