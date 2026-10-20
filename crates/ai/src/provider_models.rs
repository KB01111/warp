//! Live model-catalog discovery for [`crate::provider_presets::ProviderPreset`]s.
//!
//! Each preset provider exposes an OpenAI-shaped `GET {base_url}/models`
//! endpoint. This module performs that request with the user's API key and
//! hands the body to [`crate::provider_presets::parse_model_catalog`], which
//! keeps only the models the preset considers free.

use std::time::Duration;

use crate::provider_presets::{DiscoveredModel, ProviderPreset, parse_model_catalog};

/// Model catalogs are small; anything slower than this is a network problem the
/// user should see rather than a spinner that never resolves.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error)]
pub enum ModelDiscoveryError {
    #[error("enter an API key to load models from {provider}")]
    MissingApiKey { provider: &'static str },
    #[error("{provider} rejected this API key (HTTP {status})")]
    RejectedApiKey { provider: &'static str, status: u16 },
    #[error("{provider} returned HTTP {status}")]
    HttpStatus { provider: &'static str, status: u16 },
    #[error("could not reach {provider}: {message}")]
    Request { provider: &'static str, message: String },
    #[error("{provider} returned an invalid model list: {message}")]
    InvalidCatalog { provider: &'static str, message: String },
}

impl ModelDiscoveryError {
    /// A short, user-facing message suitable for the endpoint editor.
    pub fn user_message(&self) -> String {
        self.to_string()
    }
}

/// Fetches the provider's catalog and returns its free models.
///
/// Models keep the provider's own ordering-independent ids; callers decide how
/// many to surface (see [`crate::provider_presets::MAX_DISCOVERED_MODELS`]).
pub async fn fetch_free_models(
    preset: &'static ProviderPreset,
    api_key: &str,
) -> Result<Vec<DiscoveredModel>, ModelDiscoveryError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(ModelDiscoveryError::MissingApiKey {
            provider: preset.display_name,
        });
    }

    let response = http_client::Client::new()
        .get(preset.models_url())
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| ModelDiscoveryError::Request {
            provider: preset.display_name,
            message: error.to_string(),
        })?;

    let status = response.status();
    if status == http_client::StatusCode::UNAUTHORIZED
        || status == http_client::StatusCode::FORBIDDEN
    {
        return Err(ModelDiscoveryError::RejectedApiKey {
            provider: preset.display_name,
            status: status.as_u16(),
        });
    }
    if !status.is_success() {
        return Err(ModelDiscoveryError::HttpStatus {
            provider: preset.display_name,
            status: status.as_u16(),
        });
    }

    let body = response
        .text()
        .await
        .map_err(|error| ModelDiscoveryError::Request {
            provider: preset.display_name,
            message: error.to_string(),
        })?;

    parse_model_catalog(preset, &body).map_err(|message| ModelDiscoveryError::InvalidCatalog {
        provider: preset.display_name,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_is_reported_without_a_request() {
        let error = futures::executor::block_on(fetch_free_models(
            &crate::provider_presets::OPENROUTER,
            "  ",
        ))
        .expect_err("an empty key cannot be used");
        assert_eq!(
            error.to_string(),
            "enter an API key to load models from OpenRouter"
        );
    }

    #[test]
    fn errors_name_the_provider() {
        let error = ModelDiscoveryError::HttpStatus {
            provider: "OpenCode Zen",
            status: 503,
        };
        assert_eq!(error.user_message(), "OpenCode Zen returned HTTP 503");
    }
}
