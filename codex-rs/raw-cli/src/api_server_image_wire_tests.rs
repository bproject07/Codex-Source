use codex_api::ImageBackground;
use codex_api::ImageData;
use codex_api::ImageGenerationRequest;
use codex_api::ImageQuality;
use codex_api::ImageResponse;
use http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::DEFAULT_IMAGE_MODEL;
use super::image_generation_response;
use super::parse_image_generation_request;

#[test]
fn parses_image_generation_defaults() {
    let request = parse_image_generation_request(json!({
        "prompt": "paint a blue whale"
    }))
    .expect("parse image generation request");

    assert_eq!(
        request,
        ImageGenerationRequest {
            prompt: "paint a blue whale".to_string(),
            background: Some(ImageBackground::Auto),
            model: DEFAULT_IMAGE_MODEL.to_string(),
            n: None,
            quality: Some(ImageQuality::Auto),
            size: Some("auto".to_string()),
        }
    );
}

#[test]
fn parses_explicit_image_generation_controls() {
    let request = parse_image_generation_request(json!({
        "prompt": "paint a blue whale",
        "model": "gpt-image-1.5",
        "n": 2,
        "quality": "high",
        "size": "1536x1024",
        "background": "opaque",
        "response_format": "b64_json"
    }))
    .expect("parse image generation controls");

    assert_eq!(
        request,
        ImageGenerationRequest {
            prompt: "paint a blue whale".to_string(),
            background: Some(ImageBackground::Opaque),
            model: "gpt-image-1.5".to_string(),
            n: Some(2),
            quality: Some(ImageQuality::High),
            size: Some("1536x1024".to_string()),
        }
    );
}

#[test]
fn rejects_invalid_image_generation_requests() {
    let cases = [
        (json!({}), "prompt"),
        (json!({"prompt": " "}), "prompt"),
        (json!({"prompt": "x", "model": " "}), "model"),
        (json!({"prompt": "x", "size": " "}), "size"),
        (json!({"prompt": "x", "n": 0}), "n"),
        (json!({"prompt": "x", "n": 5}), "n"),
        (
            json!({"prompt": "x", "response_format": "url"}),
            "response_format",
        ),
        (json!({"prompt": "x", "stream": true}), "stream"),
    ];

    for (value, param) in cases {
        let error = parse_image_generation_request(value).expect_err("request must fail");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.param.as_deref(), Some(param));
    }
}

#[test]
fn renders_image_generation_response_without_copying_api_only_fields() {
    let response = image_generation_response(ImageResponse {
        created: 123,
        data: vec![
            ImageData {
                b64_json: "first".to_string(),
            },
            ImageData {
                b64_json: "second".to_string(),
            },
        ],
        background: Some(ImageBackground::Opaque),
        quality: Some(ImageQuality::High),
        size: Some("1536x1024".to_string()),
    });

    assert_eq!(
        response,
        json!({
            "created": 123,
            "data": [
                {"b64_json": "first"},
                {"b64_json": "second"}
            ],
            "background": "opaque",
            "quality": "high",
            "size": "1536x1024"
        })
    );
}
