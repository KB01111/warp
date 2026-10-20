use std::collections::HashSet;

use serde_json::Value;

use super::*;
use crate::api_keys::CustomEndpointSchema;

fn model(id: &str) -> DiscoveredModel {
    DiscoveredModel {
        id: id.to_owned(),
        display_name: None,
        context_window: None,
    }
}

#[test]
fn presets_have_unique_ids_base_urls_and_names() {
    let mut ids = HashSet::new();
    let mut urls = HashSet::new();
    let mut names = HashSet::new();
    for preset in PROVIDER_PRESETS {
        assert!(ids.insert(preset.id), "duplicate preset id: {}", preset.id);
        assert!(
            urls.insert(normalize_base_url(preset.base_url)),
            "duplicate preset base URL: {}",
            preset.base_url
        );
        assert!(
            names.insert(preset.display_name),
            "duplicate preset name: {}",
            preset.display_name
        );
    }
    assert_eq!(preset_by_id("openrouter"), Some(&OPENROUTER));
    assert_eq!(preset_by_id(" NVIDIA-BUILD "), Some(&NVIDIA_BUILD));
    assert_eq!(preset_by_id("nope"), None);
}

#[test]
fn preset_ids_are_file_safe() {
    for preset in PROVIDER_PRESETS {
        assert!(!preset.id.is_empty());
        assert!(
            preset
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
            "preset id must be file-safe: {}",
            preset.id
        );
        assert!(!preset.display_name.is_empty());
        assert!(!preset.description.is_empty());
        assert!(!preset.api_key_placeholder.is_empty());
    }
}

#[test]
fn preset_base_urls_are_https_and_have_no_trailing_slash() {
    for preset in PROVIDER_PRESETS {
        assert!(
            preset.base_url.starts_with("https://"),
            "{} must use HTTPS",
            preset.id
        );
        assert!(
            !preset.base_url.ends_with('/'),
            "{} must not end with a slash",
            preset.id
        );
        assert!(
            preset.api_key_url.starts_with("https://"),
            "{} must link to an HTTPS key page",
            preset.id
        );
        assert_eq!(
            preset.schema,
            CustomEndpointSchema::OpenaiChatCompletions,
            "{} speaks OpenAI Chat Completions",
            preset.id
        );
    }
}

#[test]
fn models_url_appends_models_once() {
    assert_eq!(
        OPENROUTER.models_url(),
        "https://openrouter.ai/api/v1/models"
    );
    assert_eq!(
        NVIDIA_BUILD.models_url(),
        "https://integrate.api.nvidia.com/v1/models"
    );
    assert_eq!(
        OPENCODE_GO.models_url(),
        "https://opencode.ai/zen/go/v1/models"
    );
}

#[test]
fn matches_base_url_tolerates_case_slashes_and_pasted_paths() {
    assert!(OPENROUTER.matches_base_url("https://openrouter.ai/api/v1"));
    assert!(OPENROUTER.matches_base_url("https://openrouter.ai/api/v1/"));
    assert!(OPENROUTER.matches_base_url("  HTTPS://OpenRouter.ai/api/v1  "));
    assert!(OPENROUTER.matches_base_url(
        "https://openrouter.ai/api/v1/chat/completions"
    ));
    assert!(!OPENROUTER.matches_base_url("https://openrouter.ai/api/v2"));
}

#[test]
fn preset_for_base_url_disambiguates_zen_from_go() {
    assert_eq!(
        preset_for_base_url("https://opencode.ai/zen/v1").map(|preset| preset.id),
        Some("opencode-zen")
    );
    assert_eq!(
        preset_for_base_url("https://opencode.ai/zen/go/v1/").map(|preset| preset.id),
        Some("opencode-go")
    );
    assert_eq!(preset_for_base_url("https://example.com/v1"), None);
}

#[test]
fn openrouter_requires_the_free_suffix() {
    assert!(OPENROUTER.is_free_model("z-ai/glm-4.5-air:free", None));
    assert!(OPENROUTER.is_free_model("Z-AI/GLM-4.5-AIR:FREE", None));
    assert!(!OPENROUTER.is_free_model("z-ai/glm-4.5-air", None));
    assert!(!OPENROUTER.is_free_model("z-ai/glm-4.5-air:free-preview", None));
}

#[test]
fn nvidia_accepts_every_model() {
    assert!(NVIDIA_BUILD.is_free_model("deepseek-ai/deepseek-v3.1", None));
    assert!(NVIDIA_BUILD.is_free_model("anything", None));
}

#[test]
fn opencode_zen_accepts_free_suffix_and_big_pickle() {
    assert!(OPENCODE_ZEN.is_free_model("nemotron-3-ultra-free", None));
    assert!(OPENCODE_ZEN.is_free_model("big-pickle", None));
    assert!(!OPENCODE_ZEN.is_free_model("gpt-5.2", None));
    assert!(!OPENCODE_GO.is_free_model("big-pickle", None));
    assert!(OPENCODE_GO.is_free_model("space-bunny-free", None));
}

#[test]
fn zero_price_metadata_marks_a_model_free() {
    let free: Value = serde_json::json!({
        "pricing": { "prompt": "0", "completion": "0.000000" }
    });
    assert!(OPENROUTER.is_free_model("vendor/paid-looking-id", Some(&free)));

    let mixed: Value = serde_json::json!({
        "pricing": { "prompt": "0", "completion": "0.000002" }
    });
    assert!(!OPENROUTER.is_free_model("vendor/paid-looking-id", Some(&mixed)));

    let zen_free: Value = serde_json::json!({ "cost": { "input": 0, "output": 0 } });
    assert!(OPENCODE_ZEN.is_free_model("some-id", Some(&zen_free)));

    let no_prices: Value = serde_json::json!({ "id": "vendor/unknown" });
    assert!(!OPENROUTER.is_free_model("vendor/unknown", Some(&no_prices)));
}

#[test]
fn parse_model_catalog_keeps_only_free_models() {
    let body = r#"{
        "data": [
            { "id": "z-ai/glm-4.5-air:free", "name": "Z.AI: GLM 4.5 Air (free)", "context_length": 131072 },
            { "id": "z-ai/glm-4.5-air", "name": "Z.AI: GLM 4.5 Air" },
            { "id": "qwen/qwen3-coder:free" }
        ]
    }"#;
    let models = parse_model_catalog(&OPENROUTER, body).expect("catalog parses");
    assert_eq!(
        models,
        vec![
            DiscoveredModel {
                id: "qwen/qwen3-coder:free".to_owned(),
                display_name: None,
                context_window: None,
            },
            DiscoveredModel {
                id: "z-ai/glm-4.5-air:free".to_owned(),
                display_name: Some("Z.AI: GLM 4.5 Air (free)".to_owned()),
                context_window: Some(131072),
            },
        ]
    );
}

#[test]
fn parse_model_catalog_accepts_a_bare_array() {
    let body = r#"[{ "id": "nvidia/llama-3.3-nemotron", "max_context_length": "131072" }]"#;
    let models = parse_model_catalog(&NVIDIA_BUILD, body).expect("catalog parses");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "nvidia/llama-3.3-nemotron");
    assert_eq!(models[0].context_window, Some(131072));
    assert_eq!(models[0].display_name, None);
}

#[test]
fn parse_model_catalog_reads_context_window_from_top_provider() {
    let body = r#"{ "data": [
        { "id": "deepseek/deepseek-v3.1:free", "top_provider": { "context_length": 64000 } }
    ] }"#;
    let models = parse_model_catalog(&OPENROUTER, body).expect("catalog parses");
    assert_eq!(models, vec![DiscoveredModel {
        id: "deepseek/deepseek-v3.1:free".to_owned(),
        display_name: None,
        context_window: Some(64000),
    }]);
}

#[test]
fn parse_model_catalog_skips_blank_and_duplicate_ids() {
    let body = r#"{ "data": [
        { "id": "a-free" },
        { "id": "a-free" },
        { "id": "" },
        { "id": "   " },
        { "name": "no id" }
    ] }"#;
    let models = parse_model_catalog(&OPENCODE_ZEN, body).expect("catalog parses");
    assert_eq!(models, vec![model("a-free")]);
}

#[test]
fn parse_model_catalog_skips_non_chat_models() {
    let body = r#"{ "data": [
        { "id": "nvidia/llama-3.3-nemotron-super-49b" },
        { "id": "nvidia/nv-embedqa-e5-v5" },
        { "id": "nvidia/nv-rerankqa-mistral-4b-v3" },
        { "id": "openai/whisper-large-v3" }
    ] }"#;
    let models = parse_model_catalog(&NVIDIA_BUILD, body).expect("catalog parses");
    assert_eq!(models, vec![model("nvidia/llama-3.3-nemotron-super-49b")]);
}

#[test]
fn parse_model_catalog_rejects_invalid_bodies() {
    assert!(parse_model_catalog(&OPENROUTER, "not json").is_err());
    assert!(parse_model_catalog(&OPENROUTER, r#"{ "models": [] }"#).is_err());
}

#[test]
fn parse_model_catalog_returns_empty_for_all_paid_catalogs() {
    let body = r#"{ "data": [ { "id": "gpt-5.2" }, { "id": "claude-opus-4-7" } ] }"#;
    let models = parse_model_catalog(&OPENCODE_ZEN, body).expect("catalog parses");
    assert!(models.is_empty());
}
