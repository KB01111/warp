//! Built-in provider presets for third-party, OpenAI-compatible inference APIs.
//!
//! Warp's custom-endpoint ("BYOE") support already lets a user talk to any
//! OpenAI Chat Completions / OpenAI Responses / Anthropic Messages endpoint, but
//! it requires hand-entering the base URL and every model id. This module
//! supplies the static half of a much shorter flow: for a handful of popular
//! providers the base URL, request schema, API-key page, and the rule that
//! identifies which of the provider's models are free are all known up front.
//!
//! The dynamic half (asking the provider's `/models` endpoint what it currently
//! serves) lives in [`crate::provider_models`]. Keeping the catalog parsing and
//! free-model filtering here makes the interesting logic pure and testable
//! without network access, and keeps it compiling on every target.
//!
//! A preset is deliberately *not* a new persisted credential type: connecting a
//! preset just creates a normal custom endpoint whose base URL matches the
//! preset, so requests, model picker entries, usage accounting, and settings all
//! flow through the existing custom-endpoint machinery.

use std::collections::HashSet;

use serde_json::Value;

use crate::api_keys::CustomEndpointSchema;

/// Id substrings that mark a catalog entry as something other than a chat
/// model. Providers (NVIDIA Build especially) publish embedding, reranking,
/// moderation, and speech models on the same `/models` endpoint, and none of
/// them can serve a Chat Completions request.
const NON_CHAT_MODEL_MARKERS: &[&str] = &["embed", "rerank", "moderation", "whisper"];

/// The maximum number of discovered models surfaced to the UI at once.
///
/// Some providers (notably NVIDIA Build) advertise well over a hundred models.
/// The endpoint editor renders one pair of text inputs per model, so populating
/// every id would make the modal enormous. The UI reports how many were hidden.
pub const MAX_DISCOVERED_MODELS: usize = 50;

/// How a provider expresses "this model costs nothing to use".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeModelRule {
    /// Every model the endpoint serves is free for key holders (rate-limited).
    All,
    /// Free models carry one of these id suffixes, are listed explicitly, or
    /// report a zero price in their catalog metadata.
    FreeIds {
        suffixes: &'static [&'static str],
        extra_ids: &'static [&'static str],
    },
}

/// A provider whose OpenAI-compatible API Warp can wire up automatically.
#[derive(Debug, PartialEq, Eq)]
pub struct ProviderPreset {
    /// Stable, file-safe identifier used by settings/telemetry-free call sites.
    pub id: &'static str,
    /// Human-readable name shown in settings and used as the endpoint name.
    pub display_name: &'static str,
    /// One-line description shown next to the "Connect" button.
    pub description: &'static str,
    /// Base URL the custom endpoint should point at (no trailing slash).
    pub base_url: &'static str,
    /// Request/response protocol spoken by the provider.
    pub schema: CustomEndpointSchema,
    /// Where the user creates an API key.
    pub api_key_url: &'static str,
    /// Placeholder shown in the modal's API-key field.
    pub api_key_placeholder: &'static str,
    /// Which of the provider's models are free to use.
    pub free_models: FreeModelRule,
}

/// OpenRouter: an OpenAI-compatible gateway in front of many upstream models.
/// Free variants are advertised with a `:free` id suffix.
pub const OPENROUTER: ProviderPreset = ProviderPreset {
    id: "openrouter",
    display_name: "OpenRouter",
    description: "One API for many models. Free variants use a \":free\" suffix.",
    base_url: "https://openrouter.ai/api/v1",
    schema: CustomEndpointSchema::OpenaiChatCompletions,
    api_key_url: "https://openrouter.ai/settings/keys",
    api_key_placeholder: "sk-or-v1-...",
    free_models: FreeModelRule::FreeIds {
        suffixes: &[":free"],
        extra_ids: &[],
    },
};

/// NVIDIA Build: hosted NIM endpoints. Every model is usable with a free
/// Developer Program key (rate-limited per model).
pub const NVIDIA_BUILD: ProviderPreset = ProviderPreset {
    id: "nvidia-build",
    display_name: "NVIDIA Build",
    description: "Hosted NIM models, free for NVIDIA Developer Program members.",
    base_url: "https://integrate.api.nvidia.com/v1",
    schema: CustomEndpointSchema::OpenaiChatCompletions,
    api_key_url: "https://build.nvidia.com/settings/api-keys",
    api_key_placeholder: "nvapi-...",
    free_models: FreeModelRule::All,
};

/// OpenCode Zen: OpenCode's curated gateway. Free models end in `-free`, with
/// `big-pickle` as the long-standing exception.
pub const OPENCODE_ZEN: ProviderPreset = ProviderPreset {
    id: "opencode-zen",
    display_name: "OpenCode Zen",
    description: "Curated coding gateway. Free models end in \"-free\".",
    base_url: "https://opencode.ai/zen/v1",
    schema: CustomEndpointSchema::OpenaiChatCompletions,
    api_key_url: "https://opencode.ai/auth",
    api_key_placeholder: "sk-...",
    free_models: FreeModelRule::FreeIds {
        suffixes: &["-free"],
        extra_ids: &["big-pickle"],
    },
};

/// OpenCode Go: OpenCode's subscription gateway. Free preview models end in
/// `-free`; the rest require an active subscription.
pub const OPENCODE_GO: ProviderPreset = ProviderPreset {
    id: "opencode-go",
    display_name: "OpenCode Go",
    description: "OpenCode's coding subscription. Free previews end in \"-free\".",
    base_url: "https://opencode.ai/zen/go/v1",
    schema: CustomEndpointSchema::OpenaiChatCompletions,
    api_key_url: "https://opencode.ai/auth",
    api_key_placeholder: "sk-...",
    free_models: FreeModelRule::FreeIds {
        suffixes: &["-free"],
        extra_ids: &[],
    },
};

/// Every preset surfaced in the Warp Agent settings page, in display order.
pub const PROVIDER_PRESETS: &[ProviderPreset] =
    &[OPENROUTER, NVIDIA_BUILD, OPENCODE_ZEN, OPENCODE_GO];

/// Looks up a preset by its stable [`ProviderPreset::id`].
pub fn preset_by_id(id: &str) -> Option<&'static ProviderPreset> {
    PROVIDER_PRESETS
        .iter()
        .find(|preset| preset.id.eq_ignore_ascii_case(id.trim()))
}

/// A single model returned by a provider's `/models` endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    /// The provider's model id, used verbatim as the custom endpoint model name.
    pub id: String,
    /// The provider's human-readable name for the model, when it differs from
    /// the id. Surfaced as the model-picker alias.
    pub display_name: Option<String>,
    /// Context window in tokens, when the catalog reports one.
    pub context_window: Option<u64>,
}

impl ProviderPreset {
    /// The URL that lists this provider's models.
    pub fn models_url(&self) -> String {
        format!("{}/models", self.base_url.trim_end_matches('/'))
    }

    /// Whether `url` points at this provider's API. Accepts a trailing slash and
    /// a pasted `/chat/completions` path, and ignores case.
    pub fn matches_base_url(&self, url: &str) -> bool {
        normalize_base_url(url) == normalize_base_url(self.base_url)
    }

    /// Whether a catalog entry describes a free model.
    ///
    /// `entry` is the raw JSON object for the model when one is available; the
    /// price metadata it may carry is used as a fallback for providers that do
    /// not encode "free" in the id.
    pub fn is_free_model(&self, id: &str, entry: Option<&Value>) -> bool {
        match self.free_models {
            FreeModelRule::All => true,
            FreeModelRule::FreeIds {
                suffixes,
                extra_ids,
            } => {
                let id_lower = id.to_ascii_lowercase();
                if extra_ids
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(id))
                {
                    return true;
                }
                if suffixes
                    .iter()
                    .any(|suffix| id_lower.ends_with(suffix.to_ascii_lowercase().as_str()))
                {
                    return true;
                }
                entry.is_some_and(reports_zero_price)
            }
        }
    }
}

/// Normalizes a user-entered base URL for comparison with a preset.
fn normalize_base_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let trimmed = trimmed
        .strip_suffix("/chat/completions")
        .unwrap_or(trimmed);
    trimmed.trim_end_matches('/').to_ascii_lowercase()
}

/// Looks up the preset a base URL belongs to, if any.
pub fn preset_for_base_url(url: &str) -> Option<&'static ProviderPreset> {
    PROVIDER_PRESETS
        .iter()
        .find(|preset| preset.matches_base_url(url))
}

/// Parses an OpenAI-shaped `/models` response and keeps only free models.
///
/// Accepts both `{"data": [...]}` (the OpenAI shape every preset provider uses)
/// and a bare top-level array, tolerates entries without an id, de-duplicates
/// repeated ids, and returns the models sorted by id for stable presentation.
pub fn parse_model_catalog(
    preset: &ProviderPreset,
    body: &str,
) -> Result<Vec<DiscoveredModel>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("the model list is not valid JSON: {error}"))?;
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .ok_or_else(|| "the model list is missing a \"data\" array".to_owned())?;

    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for entry in entries {
        let Some(id) = entry
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if is_non_chat_model(id)
            || !preset.is_free_model(id, Some(entry))
            || !seen.insert(id.to_owned())
        {
            continue;
        }
        models.push(DiscoveredModel {
            id: id.to_owned(),
            display_name: advertised_name(entry, id),
            context_window: context_window(entry),
        });
    }
    models.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(models)
}

/// Whether a model id looks like a non-chat model (embedding, reranker,
/// moderation, or speech) that cannot serve a Chat Completions request.
fn is_non_chat_model(id: &str) -> bool {
    let id_lower = id.to_ascii_lowercase();
    NON_CHAT_MODEL_MARKERS
        .iter()
        .any(|marker| id_lower.contains(marker))
}

/// Returns the catalog's display name for a model, when it adds information
/// beyond the id (OpenRouter repeats the id for some models).
fn advertised_name(entry: &Value, id: &str) -> Option<String> {
    entry
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(id))
        .map(str::to_owned)
}

/// Reads a context window from the handful of field names providers use.
fn context_window(entry: &Value) -> Option<u64> {
    let direct = entry
        .get("context_length")
        .or_else(|| entry.get("context_window"))
        .or_else(|| entry.get("max_context_length"))
        .and_then(json_u64);
    direct.or_else(|| {
        entry
            .get("top_provider")
            .and_then(|provider| provider.get("context_length"))
            .and_then(json_u64)
    })
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

/// Whether a catalog entry's price metadata reports a zero cost.
///
/// Only treats an entry as free when at least one price field exists and every
/// price field that exists is zero, so "free input, paid output" stays paid.
fn reports_zero_price(entry: &Value) -> bool {
    const PRICE_FIELDS: &[(&str, &str)] = &[
        ("pricing", "prompt"),
        ("pricing", "completion"),
        ("pricing", "input"),
        ("pricing", "output"),
        ("cost", "input"),
        ("cost", "output"),
        ("cost", "prompt"),
        ("cost", "completion"),
    ];

    let mut saw_price = false;
    for (object, field) in PRICE_FIELDS {
        let Some(price) = entry.get(*object).and_then(|object| object.get(*field)) else {
            continue;
        };
        saw_price = true;
        if !price_is_zero(price) {
            return false;
        }
    }
    saw_price
}

fn price_is_zero(price: &Value) -> bool {
    match price {
        Value::Number(number) => number.as_f64().is_some_and(|value| value == 0.),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .is_ok_and(|value| value == 0.),
        _ => false,
    }
}

#[cfg(test)]
#[path = "provider_presets_tests.rs"]
mod tests;
