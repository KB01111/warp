use ::ai::api_keys::{CustomEndpoint, CustomEndpointSchema, validate_custom_endpoint_url};
use ::ai::provider_presets::{ProviderPreset, preset_for_base_url};
use warp_editor::editor::NavigationKey;
use warpui::elements::{
    Border, ChildView, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container,
    CornerRadius, CrossAxisAlignment, Empty, Expanded, Flex, MainAxisSize, MouseStateHandle,
    ParentElement, Radius, SavePosition, ScrollTarget, ScrollToPositionMode, ScrollbarWidth,
    Shrinkable, Stack, Text,
};
use warpui::fonts::FamilyId;
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::units::Pixels;
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use crate::appearance::{Appearance, AppearanceEvent};
use crate::editor::{
    EditorView, Event as EditorEvent, PropagateAndNoOpNavigationKeys, SingleLineEditorOptions,
    TextOptions,
};
use crate::modal::{Modal, ModalViewState};
use crate::ui_components::icons::Icon;
use crate::view_components::action_button::{ActionButton, DangerSecondaryTheme};
use crate::view_components::dropdown::DropdownEvent;
use crate::view_components::{Dropdown, DropdownItem};

const LABEL_FONT_SIZE: f32 = 12.;
const INPUT_WIDTH: f32 = 480.;
const ENDPOINT_NAME_SCROLL_POSITION_ID: &str = "custom_endpoint_name";
const ENDPOINT_URL_SCROLL_POSITION_ID: &str = "custom_endpoint_url";
const API_KEY_SCROLL_POSITION_ID: &str = "custom_endpoint_api_key";
const ACTIONS_POSITION_ID: &str = "custom_endpoint_actions";

const MODEL_ROW_SPACING: f32 = 16.;
const REMOVE_MODEL_BUTTON_SPACING: f32 = 8.;
const REMOVE_MODEL_BUTTON_COL_WIDTH: f32 = 20.;
const MODAL_SCROLLBAR_WIDTH: f32 = 4.;
// Right margin shared by the scrollable content and the (non-scrollable) actions
// row. It includes the overlayed scrollbar width so the scrollable content's right
// edge (e.g. the remove-model "X" buttons) lines up with the Save/Cancel buttons,
// which have no scrollbar.
const SCROLL_CONTENT_RIGHT_MARGIN: f32 = 24. + MODAL_SCROLLBAR_WIDTH;
const MODEL_INPUT_WIDTH: f32 = (INPUT_WIDTH - MODEL_ROW_SPACING) / 2.;
fn model_row_scroll_position_id(index: usize) -> String {
    format!("custom_endpoint_model_row_{index}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomEndpointModalEvent {
    Close,
    AddEndpoint {
        name: String,
        url: String,
        api_key: String,
        schema: CustomEndpointSchema,
        models: Vec<(String, Option<String>, Option<String>)>,
    },
    SaveEndpoint {
        index: usize,
        name: String,
        url: String,
        api_key: String,
        schema: CustomEndpointSchema,
        models: Vec<(String, Option<String>, Option<String>)>,
    },
    RemoveEndpoint {
        index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomEndpointModalAction {
    Cancel,
    Save,
    AddModel,
    RemoveModel(usize),
    RemoveEndpoint,
    SetSchema(CustomEndpointSchema),
    /// Loads the free model catalog from the preset provider matching the
    /// current endpoint URL.
    FetchModels,
}

/// Progress of a live model-catalog fetch from a preset provider.
///
/// Only `Idle` is constructed on wasm (where catalog discovery is unavailable),
/// so the other variants are allowed to go unconstructed on that target.
#[cfg_attr(target_family = "wasm", allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum FetchState {
    #[default]
    Idle,
    Loading,
    /// `shown` is capped at the number of rows we are willing to materialize;
    /// `total` is how many free models the provider reported.
    Loaded { shown: usize, total: usize },
    Failed(String),
}

struct ModelRow {
    name_editor: ViewHandle<EditorView>,
    alias_editor: ViewHandle<EditorView>,
    remove_mouse_state: MouseStateHandle,
    config_key: Option<String>,
}

pub struct CustomEndpointModal {
    endpoint_name_editor: ViewHandle<EditorView>,
    endpoint_url_editor: ViewHandle<EditorView>,
    api_key_editor: ViewHandle<EditorView>,
    schema_dropdown: ViewHandle<Dropdown<CustomEndpointModalAction>>,
    schema: CustomEndpointSchema,
    model_rows: Vec<ModelRow>,
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    fetch_state: FetchState,
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    fetch_button_mouse_state: MouseStateHandle,
    cancel_button_mouse_state: MouseStateHandle,
    save_button_mouse_state: MouseStateHandle,
    add_model_button_mouse_state: MouseStateHandle,
    remove_endpoint_button: ViewHandle<ActionButton>,
    editing_index: Option<usize>,
    url_has_error: bool,
    scroll_state: ClippedScrollStateHandle,
}

impl CustomEndpointModal {
    pub fn new(
        endpoint: Option<&CustomEndpoint>,
        editing_index: Option<usize>,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        // Editor text colors are snapshotted at construction via
        // `text_colors_override`, so refresh them whenever the theme changes.
        ctx.subscribe_to_model(&Appearance::handle(ctx), |me, _, event, ctx| {
            if let AppearanceEvent::ThemeChanged = event {
                me.update_editor_text_colors(ctx);
            }
        });
        let font_family = Appearance::as_ref(ctx).ui_font_family();
        let text_colors = crate::settings_view::editor_text_colors(Appearance::as_ref(ctx));

        let endpoint_name_text_colors = text_colors.clone();
        let endpoint_name_editor = ctx.add_typed_action_view(move |ctx| {
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_family_override: Some(font_family),
                    text_colors_override: Some(endpoint_name_text_colors.clone()),
                    ..Default::default()
                },
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::Always,
                ..Default::default()
            };
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text("e.g., Zach's external models", ctx);
            if let Some(ep) = endpoint {
                editor.set_buffer_text(&ep.name, ctx);
            }
            editor
        });

        let endpoint_url_text_colors = text_colors.clone();
        let endpoint_url_editor = ctx.add_typed_action_view(move |ctx| {
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_family_override: Some(font_family),
                    text_colors_override: Some(endpoint_url_text_colors.clone()),
                    ..Default::default()
                },
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::Always,
                ..Default::default()
            };
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text("Please include 'https://'", ctx);
            if let Some(ep) = endpoint {
                editor.set_buffer_text(&ep.url, ctx);
            }
            editor
        });

        let api_key_text_colors = text_colors.clone();
        let api_key_editor = ctx.add_typed_action_view(move |ctx| {
            let options = SingleLineEditorOptions {
                is_password: true,
                text: TextOptions {
                    font_family_override: Some(font_family),
                    text_colors_override: Some(api_key_text_colors.clone()),
                    ..Default::default()
                },
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::Always,
                ..Default::default()
            };
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text("e.g., sk-...", ctx);
            if let Some(ep) = endpoint {
                editor.set_buffer_text(&ep.api_key, ctx);
            }
            editor
        });

        let schema = endpoint.map(|endpoint| endpoint.schema).unwrap_or_default();
        let schema_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            // Render the popup externally on the outermost Stack so it paints
            // after all form content and appears on top.
            dropdown.set_render_popup_externally(true, ctx);
            dropdown.set_match_menu_width_to_top_bar(true, ctx);
            dropdown.set_items(
                [
                    CustomEndpointSchema::OpenaiChatCompletions,
                    CustomEndpointSchema::OpenaiResponses,
                    CustomEndpointSchema::AnthropicMessages,
                ]
                .into_iter()
                .map(|schema| {
                    DropdownItem::new(
                        schema.display_name(),
                        CustomEndpointModalAction::SetSchema(schema),
                    )
                })
                .collect(),
                ctx,
            );
            dropdown
        });
        // Re-render the modal body when the schema dropdown opens or closes so
        // we can add/remove the popup on the outer Stack.
        ctx.subscribe_to_view(&schema_dropdown, |_, _, event, ctx| match event {
            DropdownEvent::ToggleExpanded | DropdownEvent::Close => ctx.notify(),
        });
        schema_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_selected_by_name(schema.display_name(), ctx);
        });
        let mut model_rows = Vec::new();
        if let Some(ep) = endpoint {
            for model in &ep.models {
                model_rows.push(Self::create_model_row(
                    Some(&model.name),
                    model.alias.as_deref(),
                    Some(model.config_key.clone()),
                    font_family,
                    &text_colors,
                    ctx,
                ));
            }
        }
        if model_rows.is_empty() {
            model_rows.push(Self::create_model_row(
                None,
                None,
                None,
                font_family,
                &text_colors,
                ctx,
            ));
        }

        ctx.subscribe_to_view(&endpoint_name_editor, |me, _, event, ctx| {
            me.handle_endpoint_name_event(event, ctx);
        });
        ctx.subscribe_to_view(&endpoint_url_editor, |me, _, event, ctx| {
            me.handle_endpoint_url_event(event, ctx);
        });
        // Validate initial URL (if any) so the error state is accurate on open.
        let initial_url = endpoint_url_editor.as_ref(ctx).buffer_text(ctx);
        let url_has_error = !initial_url.trim().is_empty() && validate_url(&initial_url).is_err();
        ctx.subscribe_to_view(&api_key_editor, |me, _, event, ctx| {
            me.handle_api_key_event(event, ctx);
        });
        for row in &model_rows {
            let name_editor = row.name_editor.clone();
            ctx.subscribe_to_view(&name_editor, |me, editor, event, ctx| {
                me.handle_model_editor_event(&editor, event, ctx);
            });
            let alias_editor = row.alias_editor.clone();
            ctx.subscribe_to_view(&alias_editor, |me, editor, event, ctx| {
                me.handle_model_editor_event(&editor, event, ctx);
            });
        }
        let remove_endpoint_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Remove", DangerSecondaryTheme)
                .with_icon(Icon::Trash)
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(CustomEndpointModalAction::RemoveEndpoint);
                })
        });

        Self {
            endpoint_name_editor,
            endpoint_url_editor,
            api_key_editor,
            schema_dropdown,
            schema,
            model_rows,
            fetch_state: FetchState::Idle,
            fetch_button_mouse_state: Default::default(),
            cancel_button_mouse_state: Default::default(),
            save_button_mouse_state: Default::default(),
            add_model_button_mouse_state: Default::default(),
            remove_endpoint_button,
            editing_index,
            url_has_error,
            scroll_state: Default::default(),
        }
    }

    fn create_model_row(
        name: Option<&str>,
        alias: Option<&str>,
        config_key: Option<String>,
        font_family: FamilyId,
        text_colors: &crate::editor::TextColors,
        ctx: &mut ViewContext<Self>,
    ) -> ModelRow {
        let tc = text_colors.clone();
        let name_editor = ctx.add_typed_action_view(move |ctx| {
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_family_override: Some(font_family),
                    text_colors_override: Some(tc.clone()),
                    ..Default::default()
                },
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::Always,
                ..Default::default()
            };
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text("e.g., GLM-5-FP8", ctx);
            if let Some(n) = name {
                editor.set_buffer_text(n, ctx);
            }
            editor
        });

        let tc = text_colors.clone();
        let alias_editor = ctx.add_typed_action_view(move |ctx| {
            let options = SingleLineEditorOptions {
                text: TextOptions {
                    font_family_override: Some(font_family),
                    text_colors_override: Some(tc.clone()),
                    ..Default::default()
                },
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::Always,
                ..Default::default()
            };
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text("e.g., GLM-5", ctx);
            if let Some(a) = alias {
                editor.set_buffer_text(a, ctx);
            }
            editor
        });

        ModelRow {
            name_editor,
            alias_editor,
            remove_mouse_state: Default::default(),
            config_key,
        }
    }

    pub fn prefill(
        &mut self,
        endpoint: Option<&CustomEndpoint>,
        editing_index: Option<usize>,
        ctx: &mut ViewContext<Self>,
    ) {
        self.editing_index = editing_index;
        self.scroll_state = Default::default();
        self.fetch_state = FetchState::Idle;
        self.endpoint_name_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(endpoint.map(|e| e.name.as_str()).unwrap_or(""), ctx);
        });
        self.endpoint_url_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(endpoint.map(|e| e.url.as_str()).unwrap_or(""), ctx);
        });
        let url = self.endpoint_url_editor.as_ref(ctx).buffer_text(ctx);
        self.url_has_error = !url.trim().is_empty() && validate_url(&url).is_err();
        self.api_key_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(endpoint.map(|e| e.api_key.as_str()).unwrap_or(""), ctx);
        });
        self.schema_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_selected_by_name(
                endpoint
                    .map(|endpoint| endpoint.schema)
                    .unwrap_or_default()
                    .display_name(),
                ctx,
            );
        });
        self.schema = endpoint.map(|endpoint| endpoint.schema).unwrap_or_default();
        // Rebuild model rows
        // Old model row editors will be dropped with the modal body
        self.model_rows.clear();
        let font_family = Appearance::as_ref(ctx).ui_font_family();
        let text_colors = crate::settings_view::editor_text_colors(Appearance::as_ref(ctx));
        if let Some(ep) = endpoint {
            for model in &ep.models {
                self.model_rows.push(Self::create_model_row(
                    Some(&model.name),
                    model.alias.as_deref(),
                    Some(model.config_key.clone()),
                    font_family,
                    &text_colors,
                    ctx,
                ));
            }
        }
        if self.model_rows.is_empty() {
            self.model_rows.push(Self::create_model_row(
                None,
                None,
                None,
                font_family,
                &text_colors,
                ctx,
            ));
        }
        for row in &self.model_rows {
            let name_editor = row.name_editor.clone();
            ctx.subscribe_to_view(&name_editor, |me, editor, event, ctx| {
                me.handle_model_editor_event(&editor, event, ctx);
            });
            let alias_editor = row.alias_editor.clone();
            ctx.subscribe_to_view(&alias_editor, |me, editor, event, ctx| {
                me.handle_model_editor_event(&editor, event, ctx);
            });
        }
    }

    /// Prefills the form from a built-in provider preset: name, base URL, and
    /// request schema are known up front, leaving the API key to the user. The
    /// model rows start empty so the catalog can be fetched.
    pub fn prefill_preset(&mut self, preset: &ProviderPreset, ctx: &mut ViewContext<Self>) {
        self.prefill(None, None, ctx);
        self.endpoint_name_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(preset.display_name, ctx);
        });
        self.endpoint_url_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(preset.base_url, ctx);
        });
        self.api_key_editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text("", ctx);
        });
        self.schema = preset.schema;
        self.schema_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_selected_by_name(preset.schema.display_name(), ctx);
        });
        self.url_has_error = false;
        self.fetch_state = FetchState::Idle;
        ctx.notify();
    }

    /// The preset whose base URL matches the current form, if any.
    fn preset(&self, app: &AppContext) -> Option<&'static ProviderPreset> {
        let url = self.endpoint_url_editor.as_ref(app).buffer_text(app);
        preset_for_base_url(&url)
    }

    pub fn on_open(&mut self, ctx: &mut ViewContext<Self>) {
        self.scroll_state.scroll_to(Pixels::zero());
        // Preset endpoints arrive with a name and URL already filled in, so the
        // API key is the only thing left to type.
        if self.preset(ctx).is_some() {
            self.focus_editor(&self.api_key_editor, ctx);
        } else {
            self.focus_editor(&self.endpoint_name_editor, ctx);
        }
    }

    pub fn on_close(&mut self, ctx: &mut ViewContext<Self>) {
        self.fetch_state = FetchState::Idle;
        self.endpoint_name_editor.update(ctx, |editor, ctx| {
            editor.clear_buffer_and_reset_undo_stack(ctx);
        });
        self.endpoint_url_editor.update(ctx, |editor, ctx| {
            editor.clear_buffer_and_reset_undo_stack(ctx);
        });
        self.api_key_editor.update(ctx, |editor, ctx| {
            editor.clear_buffer_and_reset_undo_stack(ctx);
        });
        for row in &self.model_rows {
            row.name_editor.update(ctx, |editor, ctx| {
                editor.clear_buffer_and_reset_undo_stack(ctx);
            });
            row.alias_editor.update(ctx, |editor, ctx| {
                editor.clear_buffer_and_reset_undo_stack(ctx);
            });
        }
    }

    /// Re-applies theme-derived text colors to every editor in the modal.
    /// Called on appearance changes since editors only snapshot their text
    /// colors at construction.
    fn update_editor_text_colors(&mut self, ctx: &mut ViewContext<Self>) {
        let text_colors = crate::settings_view::editor_text_colors(Appearance::as_ref(ctx));
        let mut editors = vec![
            self.endpoint_name_editor.clone(),
            self.endpoint_url_editor.clone(),
            self.api_key_editor.clone(),
        ];
        for row in &self.model_rows {
            editors.push(row.name_editor.clone());
            editors.push(row.alias_editor.clone());
        }
        for editor in editors {
            let colors = text_colors.clone();
            editor.update(ctx, move |editor, ctx| {
                editor.set_text_colors(colors, ctx);
            });
        }
    }

    fn save(&mut self, ctx: &mut ViewContext<Self>) {
        self.validate_url_field(ctx);
        if !self.is_valid(ctx) {
            return;
        }
        let name = self.endpoint_name_editor.as_ref(ctx).buffer_text(ctx);
        let url = self.endpoint_url_editor.as_ref(ctx).buffer_text(ctx);
        let api_key = self.api_key_editor.as_ref(ctx).buffer_text(ctx);
        let schema = self.selected_schema(ctx);
        let models: Vec<(String, Option<String>, Option<String>)> = self
            .model_rows
            .iter()
            .map(|row| {
                let name = row.name_editor.as_ref(ctx).buffer_text(ctx);
                let alias = row.alias_editor.as_ref(ctx).buffer_text(ctx);
                let alias_opt = if alias.trim().is_empty() {
                    None
                } else {
                    Some(alias)
                };
                (name, alias_opt, row.config_key.clone())
            })
            .filter(|(name, _, _)| !name.trim().is_empty())
            .collect();
        if let Some(index) = self.editing_index {
            ctx.emit(CustomEndpointModalEvent::SaveEndpoint {
                index,
                name,
                url,
                api_key,
                schema,
                models,
            });
        } else {
            ctx.emit(CustomEndpointModalEvent::AddEndpoint {
                name,
                url,
                api_key,
                schema,
                models,
            });
        }
    }

    /// Returns the schema currently selected in the dropdown.
    ///
    /// The schema dropdown renders its popup externally (see
    /// `set_render_popup_externally`), so the selection action does not bubble
    /// through the dropdown to update `self.schema` via `SetSchema`. Read the
    /// dropdown's mirrored selection directly instead, falling back to the
    /// last known schema.
    fn selected_schema(&self, ctx: &AppContext) -> CustomEndpointSchema {
        match self.schema_dropdown.as_ref(ctx).selected_action() {
            Some(CustomEndpointModalAction::SetSchema(schema)) => schema,
            _ => self.schema,
        }
    }

    fn cancel(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(CustomEndpointModalEvent::Close);
    }

    fn add_model(&mut self, ctx: &mut ViewContext<Self>) {
        let font_family = Appearance::as_ref(ctx).ui_font_family();
        let text_colors = crate::settings_view::editor_text_colors(Appearance::as_ref(ctx));
        let row = Self::create_model_row(None, None, None, font_family, &text_colors, ctx);
        // Subscribe to the new editors
        let name_editor = row.name_editor.clone();
        ctx.subscribe_to_view(&name_editor, |me, editor, event, ctx| {
            me.handle_model_editor_event(&editor, event, ctx);
        });
        let alias_editor = row.alias_editor.clone();
        ctx.subscribe_to_view(&alias_editor, |me, editor, event, ctx| {
            me.handle_model_editor_event(&editor, event, ctx);
        });
        self.model_rows.push(row);
        // ClippedScrollable clamps this to its true maximum after laying out the new row.
        // Until the form overflows, that maximum remains zero and the modal grows naturally.
        self.scroll_state.scroll_to(Pixels::new(f32::MAX));
        ctx.notify();
    }

    fn remove_model(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if index < self.model_rows.len() {
            let _row = self.model_rows.remove(index);
            ctx.notify();
        }
    }

    fn is_valid(&self, app: &AppContext) -> bool {
        let name = self.endpoint_name_editor.as_ref(app).buffer_text(app);
        let url = self.endpoint_url_editor.as_ref(app).buffer_text(app);
        let api_key = self.api_key_editor.as_ref(app).buffer_text(app);
        let has_models = self.model_rows.iter().any(|row| {
            !row.name_editor
                .as_ref(app)
                .buffer_text(app)
                .trim()
                .is_empty()
        });
        is_endpoint_form_valid(&name, &url, &api_key, has_models)
    }

    fn focus_editor(&self, editor: &ViewHandle<EditorView>, ctx: &mut ViewContext<Self>) {
        ctx.focus(editor);
        let position_id = if self.endpoint_name_editor == *editor {
            Some(ENDPOINT_NAME_SCROLL_POSITION_ID.to_string())
        } else if self.endpoint_url_editor == *editor {
            Some(ENDPOINT_URL_SCROLL_POSITION_ID.to_string())
        } else if self.api_key_editor == *editor {
            Some(API_KEY_SCROLL_POSITION_ID.to_string())
        } else {
            self.model_rows
                .iter()
                .position(|row| row.name_editor == *editor || row.alias_editor == *editor)
                .map(model_row_scroll_position_id)
        };
        if let Some(position_id) = position_id {
            self.scroll_state.scroll_to_position(ScrollTarget {
                position_id,
                mode: ScrollToPositionMode::FullyIntoView,
            });
            ctx.notify();
        }
    }

    fn focus_next_editor(&self, current: &ViewHandle<EditorView>, ctx: &mut ViewContext<Self>) {
        let mut editors: Vec<&ViewHandle<EditorView>> = vec![
            &self.endpoint_name_editor,
            &self.endpoint_url_editor,
            &self.api_key_editor,
        ];
        for row in &self.model_rows {
            editors.push(&row.name_editor);
            editors.push(&row.alias_editor);
        }
        if let Some(pos) = editors.iter().position(|e| *e == current) {
            let next = (pos + 1) % editors.len();
            self.focus_editor(editors[next], ctx);
        }
    }

    fn focus_prev_editor(&self, current: &ViewHandle<EditorView>, ctx: &mut ViewContext<Self>) {
        let mut editors: Vec<&ViewHandle<EditorView>> = vec![
            &self.endpoint_name_editor,
            &self.endpoint_url_editor,
            &self.api_key_editor,
        ];
        for row in &self.model_rows {
            editors.push(&row.name_editor);
            editors.push(&row.alias_editor);
        }
        if let Some(pos) = editors.iter().position(|e| *e == current) {
            let prev = if pos == 0 { editors.len() - 1 } else { pos - 1 };
            self.focus_editor(editors[prev], ctx);
        }
    }

    fn handle_endpoint_name_event(&mut self, event: &EditorEvent, ctx: &mut ViewContext<Self>) {
        match event {
            EditorEvent::Navigate(NavigationKey::Tab) => {
                self.focus_editor(&self.endpoint_url_editor, ctx);
            }
            EditorEvent::Navigate(NavigationKey::ShiftTab) => {
                self.focus_prev_editor(&self.endpoint_name_editor, ctx);
            }
            EditorEvent::Enter => {
                self.focus_editor(&self.endpoint_url_editor, ctx);
            }
            EditorEvent::Escape => {
                self.cancel(ctx);
            }
            EditorEvent::Edited(_) => {
                ctx.notify();
            }
            _ => {}
        }
    }

    fn handle_endpoint_url_event(&mut self, event: &EditorEvent, ctx: &mut ViewContext<Self>) {
        match event {
            EditorEvent::Navigate(NavigationKey::Tab) => {
                self.validate_url_field(ctx);
                self.focus_editor(&self.api_key_editor, ctx);
            }
            EditorEvent::Navigate(NavigationKey::ShiftTab) => {
                self.validate_url_field(ctx);
                self.focus_editor(&self.endpoint_name_editor, ctx);
            }
            EditorEvent::Enter => {
                self.validate_url_field(ctx);
                self.focus_editor(&self.api_key_editor, ctx);
            }
            EditorEvent::Escape => {
                self.cancel(ctx);
            }
            EditorEvent::Edited(_) => {
                if !self.validate_url_field(ctx) {
                    ctx.notify();
                }
            }
            _ => {}
        }
    }

    fn validate_url_field(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        let url = self.endpoint_url_editor.as_ref(ctx).buffer_text(ctx);
        let had_error = self.url_has_error;
        self.url_has_error = !url.trim().is_empty() && validate_url(&url).is_err();
        let changed = self.url_has_error != had_error;
        if changed {
            ctx.notify();
        }
        changed
    }

    fn handle_api_key_event(&mut self, event: &EditorEvent, ctx: &mut ViewContext<Self>) {
        match event {
            EditorEvent::Navigate(NavigationKey::Tab) => {
                if let Some(first_row) = self.model_rows.first() {
                    self.focus_editor(&first_row.name_editor, ctx);
                }
            }
            EditorEvent::Navigate(NavigationKey::ShiftTab) => {
                self.focus_editor(&self.endpoint_url_editor, ctx);
            }
            EditorEvent::Enter => {
                if let Some(first_row) = self.model_rows.first() {
                    self.focus_editor(&first_row.name_editor, ctx);
                }
            }
            EditorEvent::Escape => {
                self.cancel(ctx);
            }
            EditorEvent::Edited(_) => {
                ctx.notify();
            }
            _ => {}
        }
    }

    fn handle_model_editor_event(
        &mut self,
        editor: &ViewHandle<EditorView>,
        event: &EditorEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            EditorEvent::Navigate(NavigationKey::Tab) | EditorEvent::Enter => {
                self.focus_next_editor(editor, ctx);
            }
            EditorEvent::Navigate(NavigationKey::ShiftTab) => {
                self.focus_prev_editor(editor, ctx);
            }
            EditorEvent::Escape => {
                self.cancel(ctx);
            }
            EditorEvent::Edited(_) => {
                ctx.notify();
            }
            _ => {}
        }
    }
}

impl CustomEndpointModal {
    /// The "Fetch free models" button plus its status line, shown for preset
    /// endpoints (and for endpoints whose URL matches a preset).
    #[cfg(not(target_family = "wasm"))]
    fn render_fetch_controls(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let mut row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(12.);

        if matches!(self.fetch_state, FetchState::Loading) {
            row.add_child(
                Text::new_inline(
                    "Fetching free models...",
                    appearance.ui_font_family(),
                    LABEL_FONT_SIZE,
                )
                .with_color(theme.nonactive_ui_text_color().into())
                .finish(),
            );
        } else {
            let label = if matches!(self.fetch_state, FetchState::Loaded { .. }) {
                "Refresh free models"
            } else {
                "Fetch free models"
            };
            row.add_child(
                appearance
                    .ui_builder()
                    .button(
                        ButtonVariant::Secondary,
                        self.fetch_button_mouse_state.clone(),
                    )
                    .with_text_label(label.to_owned())
                    .with_style(UiComponentStyles {
                        font_size: Some(14.),
                        padding: Some(Coords::uniform(6.).left(8.).right(8.)),
                        ..Default::default()
                    })
                    .build()
                    .on_click(|ctx, _, _| {
                        ctx.dispatch_typed_action(CustomEndpointModalAction::FetchModels);
                    })
                    .finish(),
            );
        }

        match &self.fetch_state {
            FetchState::Loaded { shown, total } => {
                let message = if *total == 0 {
                    "No free models found".to_owned()
                } else if shown < total {
                    format!("Showing {shown} of {total} free models")
                } else {
                    format!("Found {total} free models")
                };
                row.add_child(
                    Text::new_inline(message, appearance.ui_font_family(), LABEL_FONT_SIZE)
                        .with_color(theme.nonactive_ui_text_color().into())
                        .finish(),
                );
            }
            FetchState::Failed(message) => {
                row.add_child(
                    Text::new_inline(
                        message.clone(),
                        appearance.ui_font_family(),
                        LABEL_FONT_SIZE,
                    )
                    .with_color(theme.ui_error_color().into())
                    .soft_wrap(true)
                    .finish(),
                );
            }
            FetchState::Idle | FetchState::Loading => {}
        }

        Container::new(row.finish())
            .with_margin_bottom(16.)
            .finish()
    }
}

#[cfg(not(target_family = "wasm"))]
impl CustomEndpointModal {
    /// Requests the matching preset's catalog using the API key in the form and
    /// replaces the model rows with the free models it returns.
    fn fetch_free_models(&mut self, ctx: &mut ViewContext<Self>) {
        if matches!(self.fetch_state, FetchState::Loading) {
            return;
        }
        let Some(preset) = self.preset(ctx) else {
            return;
        };
        let api_key = self.api_key_editor.as_ref(ctx).buffer_text(ctx);
        self.fetch_state = FetchState::Loading;
        ctx.notify();

        let api_key = api_key.trim().to_owned();
        ctx.spawn(
            async move { ::ai::provider_models::fetch_free_models(preset, &api_key).await },
            |me, result, ctx| me.handle_discovered_models(result, ctx),
        );
    }

    fn handle_discovered_models(
        &mut self,
        result: Result<
            Vec<::ai::provider_presets::DiscoveredModel>,
            ::ai::provider_models::ModelDiscoveryError,
        >,
        ctx: &mut ViewContext<Self>,
    ) {
        match result {
            Ok(models) => self.apply_discovered_models(models, ctx),
            Err(error) => {
                self.fetch_state = FetchState::Failed(error.user_message());
                ctx.notify();
            }
        }
    }

    /// Rebuilds the model rows from a discovered catalog.
    ///
    /// Config keys are carried over from any row with the same model name, so
    /// refreshing a catalog does not invalidate the user's default-model
    /// selection or execution-profile overrides.
    fn apply_discovered_models(
        &mut self,
        models: Vec<::ai::provider_presets::DiscoveredModel>,
        ctx: &mut ViewContext<Self>,
    ) {
        let existing_keys: std::collections::HashMap<String, String> = self
            .model_rows
            .iter()
            .filter_map(|row| {
                let config_key = row.config_key.clone()?;
                let name = row.name_editor.as_ref(ctx).buffer_text(ctx);
                Some((name.trim().to_owned(), config_key))
            })
            .collect();

        let total = models.len();
        let shown = total.min(::ai::provider_presets::MAX_DISCOVERED_MODELS);
        let font_family = Appearance::as_ref(ctx).ui_font_family();
        let text_colors = crate::settings_view::editor_text_colors(Appearance::as_ref(ctx));

        self.model_rows.clear();
        for model in models
            .into_iter()
            .take(::ai::provider_presets::MAX_DISCOVERED_MODELS)
        {
            let config_key = existing_keys.get(&model.id).cloned();
            let row = Self::create_model_row(
                Some(&model.id),
                model.display_name.as_deref(),
                config_key,
                font_family,
                &text_colors,
                ctx,
            );
            Self::subscribe_to_model_row(&row, ctx);
            self.model_rows.push(row);
        }
        if self.model_rows.is_empty() {
            let row = Self::create_model_row(None, None, None, font_family, &text_colors, ctx);
            Self::subscribe_to_model_row(&row, ctx);
            self.model_rows.push(row);
        }

        self.fetch_state = FetchState::Loaded { shown, total };
        ctx.notify();
    }

    /// Wires a freshly created model row's editors into the modal's navigation
    /// and change handling.
    fn subscribe_to_model_row(row: &ModelRow, ctx: &mut ViewContext<Self>) {
        let name_editor = row.name_editor.clone();
        ctx.subscribe_to_view(&name_editor, |me, editor, event, ctx| {
            me.handle_model_editor_event(&editor, event, ctx);
        });
        let alias_editor = row.alias_editor.clone();
        ctx.subscribe_to_view(&alias_editor, |me, editor, event, ctx| {
            me.handle_model_editor_event(&editor, event, ctx);
        });
    }
}

impl Entity for CustomEndpointModal {
    type Event = CustomEndpointModalEvent;
}

impl View for CustomEndpointModal {
    fn ui_name() -> &'static str {
        "CustomEndpointModal"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        let is_valid = self.is_valid(app);
        let is_editing = self.editing_index.is_some();

        let label_font_family = appearance.ui_font_family();
        let label_text_color = theme.active_ui_text_color().into();
        let label = move |text: &'static str| {
            Text::new(text, label_font_family, LABEL_FONT_SIZE)
                .with_color(label_text_color)
                .finish()
        };

        let input_style = UiComponentStyles {
            width: Some(INPUT_WIDTH),
            ..Default::default()
        };
        let button_style = UiComponentStyles {
            font_size: Some(14.),
            padding: Some(Coords::uniform(8.).left(12.).right(12.)),
            ..Default::default()
        };

        let mut column = Flex::column();

        // Description. Preset endpoints already know their name and URL, so
        // point the user at the API-key page instead of asking them to fill in
        // details we can fill in for them.
        let description = match self.preset(app) {
            Some(preset) => format!(
                "{} Paste an API key below and fetch this provider's free models. Get a key at {}. Keys are stored only on this device.",
                preset.description, preset.api_key_url
            ),
            None => "Provide your endpoint details below. You can add as many models from the endpoint as you'd like and can also provide aliases for the model picker in your input.".to_owned(),
        };
        column.add_child(
            Container::new(
                Text::new(description, appearance.ui_font_family(), LABEL_FONT_SIZE)
                    .with_color(theme.nonactive_ui_text_color().into())
                    .soft_wrap(true)
                    .finish(),
            )
            .with_margin_bottom(16.)
            .finish(),
        );
        // Request/response protocol
        column.add_child(
            Container::new(label("API schema"))
                .with_margin_bottom(4.)
                .finish(),
        );
        column.add_child(
            Container::new(ChildView::new(&self.schema_dropdown).finish())
                .with_margin_bottom(16.)
                .finish(),
        );

        // Endpoint name
        column.add_child(
            Container::new(label("Endpoint name"))
                .with_margin_bottom(4.)
                .finish(),
        );
        column.add_child(
            SavePosition::new(
                Container::new(
                    appearance
                        .ui_builder()
                        .text_input(self.endpoint_name_editor.clone())
                        .with_style(input_style)
                        .build()
                        .finish(),
                )
                .with_margin_bottom(16.)
                .finish(),
                ENDPOINT_NAME_SCROLL_POSITION_ID,
            )
            .finish(),
        );

        // Endpoint URL
        column.add_child(
            Container::new(label("Endpoint URL"))
                .with_margin_bottom(4.)
                .finish(),
        );
        let url_border_fill = if self.url_has_error {
            theme.ui_error_color().into()
        } else {
            theme.outline()
        };
        column.add_child(
            SavePosition::new(
                Container::new(
                    appearance
                        .ui_builder()
                        .text_input(self.endpoint_url_editor.clone())
                        .with_style(input_style)
                        .build()
                        .finish(),
                )
                .with_border(Border::all(1.).with_border_fill(url_border_fill))
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
                .with_margin_bottom(16.)
                .finish(),
                ENDPOINT_URL_SCROLL_POSITION_ID,
            )
            .finish(),
        );

        // API key
        column.add_child(
            Container::new(label("API key"))
                .with_margin_bottom(4.)
                .finish(),
        );
        column.add_child(
            SavePosition::new(
                Container::new(
                    appearance
                        .ui_builder()
                        .text_input(self.api_key_editor.clone())
                        .with_style(input_style)
                        .build()
                        .finish(),
                )
                .with_margin_bottom(16.)
                .finish(),
                API_KEY_SCROLL_POSITION_ID,
            )
            .finish(),
        );

        // Live catalog fetch for preset providers. Native only: model discovery
        // uses the platform HTTP client, which is unavailable on wasm.
        #[cfg(not(target_family = "wasm"))]
        if self.preset(app).is_some() {
            column.add_child(self.render_fetch_controls(app));
        }

        // Model rows
        let has_remove_model_button = self.model_rows.len() > 1;
        let mut model_labels = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(REMOVE_MODEL_BUTTON_SPACING)
            .with_child(
                Flex::row()
                    .with_spacing(MODEL_ROW_SPACING)
                    .with_child(
                        ConstrainedBox::new(label("Model name"))
                            .with_width(MODEL_INPUT_WIDTH)
                            .finish(),
                    )
                    .with_child(
                        ConstrainedBox::new(label("Model alias (optional)"))
                            .with_width(MODEL_INPUT_WIDTH)
                            .finish(),
                    )
                    .finish(),
            );
        if has_remove_model_button {
            model_labels.add_child(
                ConstrainedBox::new(Empty::new().finish())
                    .with_width(REMOVE_MODEL_BUTTON_COL_WIDTH)
                    .finish(),
            );
        }

        // Model column labels
        column.add_child(
            Container::new(model_labels.finish())
                .with_margin_bottom(4.)
                .finish(),
        );

        for (i, row) in self.model_rows.iter().enumerate() {
            let name_input = appearance
                .ui_builder()
                .text_input(row.name_editor.clone())
                .with_style(UiComponentStyles {
                    width: Some(MODEL_INPUT_WIDTH),
                    ..Default::default()
                })
                .build()
                .finish();

            let alias_input = appearance
                .ui_builder()
                .text_input(row.alias_editor.clone())
                .with_style(UiComponentStyles {
                    width: Some(MODEL_INPUT_WIDTH),
                    ..Default::default()
                })
                .build()
                .finish();

            let remove_button = if self.model_rows.len() > 1 {
                appearance
                    .ui_builder()
                    .close_button(20., row.remove_mouse_state.clone())
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(CustomEndpointModalAction::RemoveModel(i));
                    })
                    .finish()
            } else {
                Empty::new().finish()
            };
            let model_inputs = Flex::row()
                .with_spacing(MODEL_ROW_SPACING)
                .with_child(name_input)
                .with_child(alias_input)
                .finish();

            let mut row = Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(REMOVE_MODEL_BUTTON_SPACING)
                .with_child(model_inputs);
            if has_remove_model_button {
                row.add_child(
                    ConstrainedBox::new(remove_button)
                        .with_width(REMOVE_MODEL_BUTTON_COL_WIDTH)
                        .finish(),
                );
            }
            let row = row.finish();

            column.add_child(
                SavePosition::new(
                    Container::new(row).with_margin_bottom(12.).finish(),
                    &model_row_scroll_position_id(i),
                )
                .finish(),
            );
        }

        let add_model_button = appearance
            .ui_builder()
            .button(
                ButtonVariant::Secondary,
                self.add_model_button_mouse_state.clone(),
            )
            .with_text_label("+ Add model".to_string())
            .with_style(UiComponentStyles {
                font_size: Some(14.),
                padding: Some(Coords::uniform(6.).left(8.).right(8.)),
                ..Default::default()
            })
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(CustomEndpointModalAction::AddModel);
            })
            .finish();

        column.add_child(
            Container::new(add_model_button)
                .with_margin_bottom(24.)
                .finish(),
        );

        // Bottom buttons row
        let mut buttons_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        // Remove button (only when editing)
        if is_editing {
            buttons_row.add_child(ChildView::new(&self.remove_endpoint_button).finish());
        }

        buttons_row.add_child(Expanded::new(1., Empty::new().finish()).finish());

        buttons_row.add_child(
            appearance
                .ui_builder()
                .button(
                    ButtonVariant::Secondary,
                    self.cancel_button_mouse_state.clone(),
                )
                .with_text_label("Cancel".to_string())
                .with_style(button_style)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CustomEndpointModalAction::Cancel);
                })
                .finish(),
        );

        let mut save_button = appearance
            .ui_builder()
            .button(ButtonVariant::Accent, self.save_button_mouse_state.clone())
            .with_text_label(if is_editing {
                "Save".to_string()
            } else {
                "Add endpoint".to_string()
            })
            .with_style(button_style);
        if !is_valid {
            save_button = save_button.disabled();
        }

        buttons_row.add_child(
            Container::new(
                save_button
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(CustomEndpointModalAction::Save);
                    })
                    .finish(),
            )
            .with_margin_left(12.)
            .finish(),
        );

        let scrollable_content = ClippedScrollable::vertical(
            self.scroll_state.clone(),
            Container::new(column.finish())
                .with_margin_right(SCROLL_CONTENT_RIGHT_MARGIN)
                .finish(),
            ScrollbarWidth::Custom(MODAL_SCROLLBAR_WIDTH),
            theme.nonactive_ui_text_color().into(),
            theme.active_ui_text_color().into(),
            warpui::elements::Fill::None,
        )
        .with_overlayed_scrollbar()
        .with_padding_start(0.)
        .with_padding_end(0.)
        .finish();
        let buttons_row = SavePosition::new(
            Container::new(buttons_row.finish())
                .with_margin_right(SCROLL_CONTENT_RIGHT_MARGIN)
                .finish(),
            ACTIONS_POSITION_ID,
        )
        .for_single_frame()
        .finish();

        // Render the schema dropdown popup at the outermost Stack level so it
        // paints after all form content and appears on top of sibling fields.
        let schema_popup = self.schema_dropdown.as_ref(app).render_menu_as_overlay();
        let mut outer_stack = Stack::new();
        outer_stack.add_child(
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(Shrinkable::new(1., scrollable_content).finish())
                .with_child(buttons_row)
                .finish(),
        );
        if let Some((popup, positioning)) = schema_popup {
            outer_stack.add_positioned_overlay_child(popup, positioning);
        }
        outer_stack.finish()
    }
}

fn validate_url(url: &str) -> Result<(), &'static str> {
    if url.trim().is_empty() {
        return Ok(());
    }
    validate_custom_endpoint_url(url)
}

fn is_endpoint_form_valid(name: &str, url: &str, api_key: &str, has_models: bool) -> bool {
    !name.trim().is_empty()
        && !url.trim().is_empty()
        && !api_key.trim().is_empty()
        && has_models
        && validate_url(url).is_ok()
}

impl TypedActionView for CustomEndpointModal {
    type Action = CustomEndpointModalAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            CustomEndpointModalAction::Cancel => self.cancel(ctx),
            CustomEndpointModalAction::Save => self.save(ctx),
            CustomEndpointModalAction::AddModel => self.add_model(ctx),
            CustomEndpointModalAction::RemoveModel(index) => self.remove_model(*index, ctx),
            CustomEndpointModalAction::RemoveEndpoint => {
                if let Some(index) = self.editing_index {
                    ctx.emit(CustomEndpointModalEvent::RemoveEndpoint { index });
                }
            }
            CustomEndpointModalAction::SetSchema(schema) => {
                self.schema = *schema;
                ctx.notify();
            }
            CustomEndpointModalAction::FetchModels => {
                #[cfg(not(target_family = "wasm"))]
                self.fetch_free_models(ctx);
            }
        }
    }
}

#[cfg(test)]
#[path = "custom_inference_modal_tests.rs"]
mod tests;

pub struct CustomEndpointModalViewState {
    state: ModalViewState<Modal<CustomEndpointModal>>,
}

impl CustomEndpointModalViewState {
    pub fn new(state: ModalViewState<Modal<CustomEndpointModal>>) -> Self {
        Self { state }
    }

    pub fn is_open(&self) -> bool {
        self.state.is_open()
    }

    pub fn render(&self) -> Box<dyn Element> {
        self.state.render()
    }

    pub fn set_title<T: View>(&mut self, title: Option<String>, ctx: &mut ViewContext<T>) {
        self.state.view.update(ctx, |modal, ctx| {
            modal.set_title(title);
            ctx.notify();
        });
    }

    pub fn prefill<T: View>(
        &mut self,
        endpoint: Option<&CustomEndpoint>,
        editing_index: Option<usize>,
        ctx: &mut ViewContext<T>,
    ) {
        self.state.view.update(ctx, |modal, ctx| {
            modal.body().update(ctx, |body, ctx| {
                body.prefill(endpoint, editing_index, ctx);
            });
        });
    }

    pub fn prefill_preset<T: View>(&mut self, preset: &ProviderPreset, ctx: &mut ViewContext<T>) {
        self.state.view.update(ctx, |modal, ctx| {
            modal.body().update(ctx, |body, ctx| {
                body.prefill_preset(preset, ctx);
            });
        });
    }

    pub fn open<T: View>(&mut self, ctx: &mut ViewContext<T>) {
        self.state.open();
        self.state.view.update(ctx, |modal, ctx| {
            modal.body().update(ctx, |body, ctx| {
                body.on_open(ctx);
            });
        });
    }

    pub fn close<T: View>(&mut self, ctx: &mut ViewContext<T>) {
        self.state.close();
        self.state.view.update(ctx, |modal, ctx| {
            modal.body().update(ctx, |body, ctx| {
                body.on_close(ctx);
            });
        });
    }
}
