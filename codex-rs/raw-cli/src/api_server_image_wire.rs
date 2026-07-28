use codex_api::ImageBackground;
use codex_api::ImageGenerationRequest;
use codex_api::ImageQuality;
use codex_api::ImageResponse;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;

use crate::api_server_wire::ApiError;

pub(crate) const DEFAULT_IMAGE_MODEL: &str = "gpt-image-2";
const MAX_IMAGES_PER_REQUEST: u64 = 4;

type WireResult<T> = Result<T, ApiError>;

#[derive(Deserialize)]
struct ImageGenerationDto {
    prompt: Option<String>,
    model: Option<String>,
    n: Option<u64>,
    quality: Option<ImageQuality>,
    size: Option<String>,
    background: Option<ImageBackground>,
    response_format: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

pub(crate) fn parse_image_generation_request(value: Value) -> WireResult<ImageGenerationRequest> {
    let dto: ImageGenerationDto = serde_json::from_value(value)
        .map_err(|error| ApiError::invalid(format!("Invalid request body: {error}"), None))?;
    reject_extra(&dto.extra)?;

    let prompt = dto
        .prompt
        .filter(|prompt| !prompt.trim().is_empty())
        .ok_or_else(|| invalid("prompt", "'prompt' must be a non-empty string."))?;
    let model = dto.model.unwrap_or_else(|| DEFAULT_IMAGE_MODEL.to_string());
    if model.trim().is_empty() {
        return Err(invalid("model", "'model' must not be empty."));
    }
    if dto.size.as_ref().is_some_and(|size| size.trim().is_empty()) {
        return Err(invalid("size", "'size' must not be empty."));
    }
    if dto
        .n
        .is_some_and(|n| !(1..=MAX_IMAGES_PER_REQUEST).contains(&n))
    {
        return Err(invalid(
            "n",
            format!("'n' must be between 1 and {MAX_IMAGES_PER_REQUEST}."),
        ));
    }
    if dto
        .response_format
        .as_deref()
        .is_some_and(|format| format != "b64_json")
    {
        return Err(invalid(
            "response_format",
            "Only 'b64_json' response format is supported.",
        ));
    }

    Ok(ImageGenerationRequest {
        prompt,
        background: Some(dto.background.unwrap_or(ImageBackground::Auto)),
        model,
        n: dto.n,
        quality: Some(dto.quality.unwrap_or(ImageQuality::Auto)),
        size: Some(dto.size.unwrap_or_else(|| "auto".to_string())),
    })
}

pub(crate) fn image_generation_response(response: ImageResponse) -> Value {
    let mut object = Map::new();
    object.insert("created".to_string(), Value::from(response.created));
    object.insert(
        "data".to_string(),
        Value::Array(
            response
                .data
                .into_iter()
                .map(|image| {
                    Value::Object(Map::from_iter([(
                        "b64_json".to_string(),
                        Value::String(image.b64_json),
                    )]))
                })
                .collect(),
        ),
    );
    if let Some(background) = response.background {
        object.insert(
            "background".to_string(),
            serde_json::to_value(background)
                .unwrap_or_else(|error| panic!("image background should serialize: {error}")),
        );
    }
    if let Some(quality) = response.quality {
        object.insert(
            "quality".to_string(),
            serde_json::to_value(quality)
                .unwrap_or_else(|error| panic!("image quality should serialize: {error}")),
        );
    }
    if let Some(size) = response.size {
        object.insert("size".to_string(), Value::String(size));
    }
    Value::Object(object)
}

fn reject_extra(extra: &Map<String, Value>) -> WireResult<()> {
    if let Some((field, _)) = extra.iter().find(|(_, value)| !value.is_null()) {
        return Err(ApiError {
            code: Some("unsupported_parameter".to_string()),
            ..invalid(field, format!("Unsupported parameter: '{field}'."))
        });
    }
    Ok(())
}

fn invalid(param: &str, message: impl Into<String>) -> ApiError {
    ApiError::invalid(message, Some(param))
}

#[cfg(test)]
#[path = "api_server_image_wire_tests.rs"]
mod tests;
