# Tech

## Registry

`crates/ai/src/provider_presets.rs` owns the static half:

- `ProviderPreset` describes one provider: stable `id`, `display_name`,
  `description`, `base_url`, `schema`, `api_key_url`, key placeholder, and a
  `FreeModelRule`.
- `PROVIDER_PRESETS` lists them in display order; `preset_by_id` and
  `preset_for_base_url` resolve a preset.
- `ProviderPreset::matches_base_url` normalizes case, trailing slashes, and a
  pasted `/chat/completions` path before comparing, so hand-edited endpoint URLs
  still match.
- `parse_model_catalog` parses the OpenAI-shaped `GET {base_url}/models`
  response (also accepting a bare array), drops entries without an id,
  de-duplicates, filters to free models, and sorts by id.
- Catalog entries whose ids look like non-chat models (`embed`, `rerank`,
  `moderation`, `whisper`) are dropped, since they cannot serve a Chat
  Completions request.
- `FreeModelRule::All` treats every model as free (NVIDIA Build);
  `FreeModelRule::FreeIds` matches id suffixes and named ids, falling back to
  zero-price metadata (`pricing.*` or `cost.*`) when the catalog reports it.

The module is pure data plus parsing, so it compiles on every target and its
tests need no network.

## Discovery

`crates/ai/src/provider_models.rs` (native only, gated at `lib.rs` because
`ai`'s `http_client` dependency is native-only) performs:

1. `GET {base_url}/models` with `Authorization: Bearer <key>`, a 20 s timeout,
   and `Accept: application/json`.
2. Status mapping: 401/403 → `RejectedApiKey`, other non-2xx → `HttpStatus`,
   transport failures → `Request`.
3. `parse_model_catalog` for the body; parse failures → `InvalidCatalog`.

`ModelDiscoveryError` renders a short user-facing message via
`user_message()`.

## UI

`app/src/settings_view/warp_agent_page.rs`:

- `ApiKeysWidget` holds one `MouseStateHandle` per preset for its rendered
  Connect/Edit buttons.
- `render_provider_presets` draws one card per preset inside the Custom
  Inference section and dispatches `WarpAgentPageAction::OpenProviderPresetModal(index)`.
  The button reads `Edit` when an endpoint with a matching base URL exists.
- `WarpAgentPageView::show_provider_preset_modal` reuses the existing
  `CustomEndpointModalViewState`, editing the matching endpoint when present and
  otherwise prefilling from the preset. It is gated by
  `can_use_custom_inference_controls`, like every other endpoint action.

`app/src/settings_view/custom_inference_modal.rs`:

- `prefill_preset` fills name/URL/schema and focuses the API key field (via
  `on_open`, which detects the preset from the URL).
- `preset()` maps the current URL to a preset; this drives both the preset
  description and the availability of the fetch controls.
- `FetchState` (`Idle`/`Loading`/`Loaded`/`Failed`) drives the "Fetch free
  models" / "Refresh free models" button and status line. The controls are
  native only.
- `fetch_free_models` spawns discovery on the view's executor;
  `apply_discovered_models` rebuilds the model rows, carrying `config_key`s over
  by model name so refreshing a catalog does not break saved model selections.
  Advertised display names become the model aliases shown in the picker.
- Only `MAX_DISCOVERED_MODELS` rows are materialized to keep the editor usable
  for catalogs with 100+ entries.

## Tests

- `crates/ai/src/provider_presets_tests.rs`: registry invariants (unique ids,
  HTTPS URLs, no trailing slashes), URL matching (including Zen vs. Go),
  per-provider free-model rules, zero-price metadata, and catalog parsing
  (dedupe, blank ids, missing `data`, invalid JSON, context-window fields).
- `crates/ai/src/provider_models.rs` unit tests cover the empty-key short
  circuit and error rendering.
- `app/src/settings_view/custom_inference_modal_tests.rs`: preset prefill fills
  the provider details, points at the preset, and resets fetch state when the
  form is reused.
