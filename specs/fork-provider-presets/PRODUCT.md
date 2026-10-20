# Provider presets for free models (fork feature)

## Summary

Add one-click setup for popular third-party, OpenAI-compatible inference
providers — OpenRouter, NVIDIA Build, OpenCode Zen, and OpenCode Go — and load
each provider's current free-model catalog from its own `/models` API instead of
requiring users to type model ids by hand.

This is a fork-local convenience layer on top of the generic custom-endpoint
capability specified in `APP-5380`. It intentionally relaxes that spec's "no
provider-specific behavior" and "no model discovery" non-goals for this fork.

## Goals

- Surface the four providers in Warp Agent settings with a single "Connect"
  button each.
- Prefill the endpoint name, base URL, and request schema so connecting a
  provider only needs an API key.
- Fetch the provider's live model catalog and keep only the models that are free
  to use, so the list never goes stale as providers rotate models.
- Keep every credential on the device, reusing the existing custom-endpoint
  secure-storage path.

## Non-goals

- A new credential type, request path, or server-side provider integration.
- Automatic background catalog refresh. Discovery runs on demand from the
  endpoint editor.
- Provider-specific request headers (for example OpenRouter attribution headers).
- Anonymous/keyless access: a key is still required, as for any custom endpoint.
- TUI preset UI. Preset endpoints appear in the TUI as ordinary custom
  endpoints once connected.

## Behavior

1. Warp Agent settings, in the Custom Inference section, lists OpenRouter,
   NVIDIA Build, OpenCode Zen, and OpenCode Go whenever custom inference is
   enabled for the workspace.
2. Selecting a provider opens the existing endpoint editor, titled
   `Connect <provider>`, with the endpoint name, base URL, and schema
   prefilled and the API key field focused.
3. For endpoints whose URL matches a preset, the editor offers
   "Fetch free models". The fetch sends the entered key to
   `{base_url}/models`, keeps the free models, and replaces the model rows with
   up to 50 of them (the editor reports how many were hidden).
4. Free models are identified per provider: an id suffix (`:free` for
   OpenRouter, `-free` for OpenCode) with named exceptions (`big-pickle`), a
   zero price in the catalog metadata where available, or every model for
   NVIDIA Build's free Developer Program endpoints.
5. Saving creates or updates a regular custom endpoint, so model-picker entries,
   usage accounting, execution profiles, and requests behave exactly as they do
   for hand-configured endpoints.
6. Selecting a provider that is already connected opens that endpoint for
   editing rather than creating a duplicate.
7. Discovery failures surface inline in the editor (bad key, HTTP error,
   unreachable host, unparsable catalog) and leave the existing model rows
   intact so the user can still configure models manually.
8. Presets use OpenAI Chat Completions. A provider whose free catalog mixes
   protocols (for example an OpenCode model that only serves the Responses
   API) may list a model that fails at request time; the schema stays
   selectable in the editor and individual model rows can be removed.
