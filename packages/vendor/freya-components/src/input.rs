use std::{
    borrow::Cow,
    cell::{Ref, RefCell},
    rc::Rc,
};

use freya_core::{
    elements::paragraph::{ParagraphCursorExt, ParagraphHolderInner},
    prelude::*,
};
use freya_edit::*;
use torin::{
    gaps::Gaps,
    prelude::{Alignment, Area, Content, CursorPoint, Direction, Point2D, Vector2D},
    size::Size,
};
use tracing::warn;

use crate::{
    cursor_blink::use_cursor_blink,
    define_theme, get_theme,
    scrollviews::{ScrollConfig, ScrollView, use_scroll_controller},
};

define_theme! {
    for = Input;
    theme_field = theme_layout;

    %[component]
    pub InputLayout {
        %[fields]
        corner_radius: CornerRadius,
        padding: Gaps,
    }
}

define_theme! {
    for = Input;
    theme_field = theme_colors;

    %[component]
    pub InputColors {
        %[fields]
        background: Color,
        focus_background: Color,
        border_fill: Color,
        focus_border_fill: Color,
        color: Color,
        placeholder_color: Color,
    }
}

#[derive(Clone, PartialEq)]
pub enum InputStyleVariant {
    Normal,
    Filled,
    Flat,
}

#[derive(Clone, PartialEq)]
pub enum InputLayoutVariant {
    Normal,
    Compact,
    Expanded,
}

#[derive(Default, Clone, Copy, PartialEq)]
pub enum InputMode {
    #[default]
    Shown,
    Hidden(char),
}

impl InputMode {
    pub fn new_password() -> Self {
        Self::Hidden('*')
    }
}

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum InputStatus {
    /// Default state.
    #[default]
    Idle,
    /// Pointer is hovering the input.
    Hovering,
}

#[derive(Clone)]
pub struct InputValidator {
    valid: Rc<RefCell<bool>>,
    text: Rc<RefCell<String>>,
}

impl InputValidator {
    pub fn new(text: String) -> Self {
        Self {
            valid: Rc::new(RefCell::new(true)),
            text: Rc::new(RefCell::new(text)),
        }
    }
    pub fn text(&'_ self) -> Ref<'_, String> {
        self.text.borrow()
    }
    pub fn set_valid(&self, is_valid: bool) {
        *self.valid.borrow_mut() = is_valid;
    }
    pub fn is_valid(&self) -> bool {
        *self.valid.borrow()
    }
}

/// Small box to write some text.
///
/// ## **Normal**
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     let value = use_state(String::new);
///     Input::new(value).placeholder("Type here")
/// }
/// ```
/// ## **Filled**
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     let value = use_state(String::new);
///     Input::new(value).placeholder("Type here").filled()
/// }
/// ```
/// ## **Flat**
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     let value = use_state(String::new);
///     Input::new(value).placeholder("Type here").flat()
/// }
/// ```
///
/// See the [interactive components demo](https://freyaui.dev/demo).
#[derive(Clone, PartialEq)]
pub struct Input {
    pub(crate) theme_colors: Option<InputColorsThemePartial>,
    pub(crate) theme_layout: Option<InputLayoutThemePartial>,
    value: Writable<String>,
    placeholder: Option<Cow<'static, str>>,
    on_validate: Option<EventHandler<InputValidator>>,
    on_submit: Option<EventHandler<String>>,
    mode: InputMode,
    auto_focus: bool,
    width: Size,
    height: Size,
    multiline: bool,
    enabled: bool,
    key: DiffKey,
    style_variant: InputStyleVariant,
    layout_variant: InputLayoutVariant,
    text_align: TextAlign,
    font_family: Option<&'static str>,
    a11y_id: Option<AccessibilityId>,
    leading: Option<Element>,
    trailing: Option<Element>,
    on_pre_key_down: Option<Callback<Event<KeyboardEventData>, bool>>,
    command: Option<State<Option<InputCommand>>>,
    probe: Option<State<InputProbe>>,
    request: Option<State<Option<InputRequest>>>,
    keep_selection: bool,
    keep_focus_in: Vec<Area>,
}

/// An edit that runs on the text as if its keyboard shortcut was pressed.
/// Set it through [`Input::command`]; the input clears it after running it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputCommand {
    SelectAll,
    Copy,
    Cut,
    Paste,
}

/// A caret or selection end in window coordinates, used to place edit UI beside the text.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct InputCaret {
    pub x: f32,
    pub top: f32,
    pub bottom: f32,
}

/// What an [`Input`] reports about its layout through [`Input::probe`].
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct InputProbe {
    /// Whether the input currently has keyboard focus.
    pub focused: bool,
    /// The text area in window coordinates.
    pub area: Area,
    /// The caret, when focused and not showing a placeholder.
    pub caret: Option<InputCaret>,
    /// The selected range as (start, end) carets in reading order, when a range is selected.
    pub selection: Option<(InputCaret, InputCaret)>,
}

/// An edit the surrounding UI asks of an [`Input`] through [`Input::request`]. The input clears it after running it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputRequest {
    /// Select the word under this window point.
    SelectWordAt { x: f32, y: f32 },
    /// Select the word at the caret, or the one just before it when the caret ends a word.
    SelectWordAtCursor,
    /// Drag the start or end of the selection with the pointer. The opposite end stays fixed until the pointer is released.
    DragSelectionEnd { start: bool },
}

/// Runs one key edit on `editable` and writes the text to `value`, unless
/// `on_validate` rejects it. Returns `true` when the edit changed the history.
fn apply_key_edit(
    editable: &mut UseEditable,
    value: &mut Writable<String>,
    on_validate: Option<&EventHandler<InputValidator>>,
    key: &Key,
    modifiers: Modifiers,
    holder: &ParagraphHolder,
) -> bool {
    let previous_history_version = editable.editor().peek().editor_history().version;
    editable.process_event(EditableEvent::KeyDown {
        key,
        modifiers,
        editor_line: Some(EditorLine::SingleParagraph),
        holder: Some(holder),
    });
    let text = editable.editor().read().committed_text();

    let apply_change = match on_validate {
        Some(on_validate) => {
            let mut editor = editable.editor_mut().write();
            let validator = InputValidator::new(text.clone());
            on_validate.call(validator.clone());
            if !validator.is_valid() {
                if let Some(selection) = editor.undo() {
                    *editor.selection_mut() = selection;
                }
                editor.editor_history_mut().clear_redos();
            }
            validator.is_valid()
        }
        None => true,
    };

    if apply_change {
        *value.write() = text;
    }
    editable.editor().peek().editor_history().version != previous_history_version
}

impl KeyExt for Input {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Input {
    pub fn new(value: impl Into<Writable<String>>) -> Self {
        Input {
            theme_colors: None,
            theme_layout: None,
            value: value.into(),
            placeholder: None,
            on_validate: None,
            on_submit: None,
            mode: InputMode::default(),
            auto_focus: false,
            width: Size::px(150.),
            height: Size::default(),
            multiline: false,
            enabled: true,
            key: DiffKey::default(),
            style_variant: InputStyleVariant::Normal,
            layout_variant: InputLayoutVariant::Normal,
            text_align: TextAlign::default(),
            font_family: None,
            a11y_id: None,
            leading: None,
            trailing: None,
            on_pre_key_down: None,
            command: None,
            probe: None,
            request: None,
            keep_selection: false,
            keep_focus_in: Vec::new(),
        }
    }

    pub fn enabled(mut self, enabled: impl Into<bool>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<Cow<'static, str>>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn on_validate(mut self, on_validate: impl Into<EventHandler<InputValidator>>) -> Self {
        self.on_validate = Some(on_validate.into());
        self
    }

    /// Run `command` on the text when the state is set to it, then clear the state.
    pub fn command(mut self, command: State<Option<InputCommand>>) -> Self {
        self.command = Some(command);
        self
    }

    /// Report focus, caret and selection geometry through `probe`. It is updated as they change.
    pub fn probe(mut self, probe: State<InputProbe>) -> Self {
        self.probe = Some(probe);
        self
    }

    /// Run `request` when the state is set to it, then clear the state.
    pub fn request(mut self, request: State<Option<InputRequest>>) -> Self {
        self.request = Some(request);
        self
    }

    /// Keep the selection highlighted while the input is not focused, as while its own menu is open.
    pub fn keep_selection(mut self, keep: bool) -> Self {
        self.keep_selection = keep;
        self
    }

    /// Window areas whose touch taps are not taps outside this input, such as its edit bar, so a
    /// tap on one keeps the focus.
    pub fn keep_focus_in(mut self, areas: Vec<Area>) -> Self {
        self.keep_focus_in = areas;
        self
    }

    pub fn on_submit(mut self, on_submit: impl Into<EventHandler<String>>) -> Self {
        self.on_submit = Some(on_submit.into());
        self
    }

    pub fn mode(mut self, mode: InputMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn auto_focus(mut self, auto_focus: impl Into<bool>) -> Self {
        self.auto_focus = auto_focus.into();
        self
    }

    pub fn width(mut self, width: impl Into<Size>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Size>) -> Self {
        self.height = height.into();
        self
    }

    /// Allow writing multiple lines, `Enter` inserts a line break instead of submitting.
    /// Combine it with [Self::height] to get a scrollable input of a bounded size.
    pub fn multiline(mut self, multiline: impl Into<bool>) -> Self {
        self.multiline = multiline.into();
        self
    }

    pub fn theme_colors(mut self, theme: InputColorsThemePartial) -> Self {
        self.theme_colors = Some(theme);
        self
    }

    pub fn theme_layout(mut self, theme: InputLayoutThemePartial) -> Self {
        self.theme_layout = Some(theme);
        self
    }

    pub fn text_align(mut self, text_align: impl Into<TextAlign>) -> Self {
        self.text_align = text_align.into();
        self
    }

    /// Sets the family the text is drawn in, such as a monospace face for keys.
    pub fn font_family(mut self, family: &'static str) -> Self {
        self.font_family = Some(family);
        self
    }

    pub fn style_variant(mut self, style_variant: impl Into<InputStyleVariant>) -> Self {
        self.style_variant = style_variant.into();
        self
    }

    pub fn layout_variant(mut self, layout_variant: impl Into<InputLayoutVariant>) -> Self {
        self.layout_variant = layout_variant.into();
        self
    }

    /// Shortcut for [Self::style_variant] with [InputStyleVariant::Filled].
    pub fn filled(self) -> Self {
        self.style_variant(InputStyleVariant::Filled)
    }

    /// Shortcut for [Self::style_variant] with [InputStyleVariant::Flat].
    pub fn flat(self) -> Self {
        self.style_variant(InputStyleVariant::Flat)
    }

    /// Shortcut for [Self::layout_variant] with [InputLayoutVariant::Compact].
    pub fn compact(self) -> Self {
        self.layout_variant(InputLayoutVariant::Compact)
    }

    /// Shortcut for [Self::layout_variant] with [InputLayoutVariant::Expanded].
    pub fn expanded(self) -> Self {
        self.layout_variant(InputLayoutVariant::Expanded)
    }

    pub fn a11y_id(mut self, a11y_id: impl Into<AccessibilityId>) -> Self {
        self.a11y_id = Some(a11y_id.into());
        self
    }

    /// Optional element rendered before the text input.
    pub fn leading(mut self, leading: impl Into<Element>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    /// Optional element rendered after the text input.
    pub fn trailing(mut self, trailing: impl Into<Element>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }

    /// Sets a pre-handler called for each key event. Return `true` to let the input process it,
    /// `false` to skip. The callback may call `stop_propagation()` / `prevent_default()` directly.
    pub fn on_pre_key_down(
        mut self,
        on_pre_key_down: impl Into<Callback<Event<KeyboardEventData>, bool>>,
    ) -> Self {
        self.on_pre_key_down = Some(on_pre_key_down.into());
        self
    }
}

impl CornerRadiusExt for Input {
    fn with_corner_radius(self, corner_radius: f32) -> Self {
        self.corner_radius(corner_radius)
    }
}

impl Component for Input {
    fn render(&self) -> impl IntoElement {
        let a11y_id = use_hook(|| self.a11y_id.unwrap_or_else(AccessibilityId::new_unique));
        let focus = use_focus(a11y_id);
        let holder = use_state(ParagraphHolder::default);
        let mut area = use_state(Area::default);
        let mut viewport_area = use_state(Area::default);
        let mut outer_area = use_state(Area::default);
        let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut status = use_state(InputStatus::default);
        let is_masked = matches!(self.mode, InputMode::Hidden(_));
        let mut editable = use_editable(
            || self.value.read().to_string(),
            move || {
                EditableConfig::new()
                    .with_allow_write_clipboard(!is_masked)
                    .with_select_all_on_double_click(is_masked)
            },
        );
        // An autofocused field that already holds text starts with the caret at its end, so typing appends.
        let start_at_end = self.auto_focus;
        use_hook(move || {
            if start_at_end {
                let mut editor = editable.editor_mut().write();
                let end = editor.rope().len_chars();
                editor.move_cursor_to(end);
            }
        });
        let mut is_dragging = use_state(|| false);
        let mut value = self.value.clone();

        let theme_colors = match self.style_variant {
            InputStyleVariant::Normal => {
                get_theme!(&self.theme_colors, InputColorsThemePreference, "input")
            }
            InputStyleVariant::Filled => get_theme!(
                &self.theme_colors,
                InputColorsThemePreference,
                "filled_input"
            ),
            InputStyleVariant::Flat => {
                get_theme!(&self.theme_colors, InputColorsThemePreference, "flat_input")
            }
        };
        let theme_layout = match self.layout_variant {
            InputLayoutVariant::Normal => get_theme!(
                &self.theme_layout,
                InputLayoutThemePreference,
                "input_layout"
            ),
            InputLayoutVariant::Compact => get_theme!(
                &self.theme_layout,
                InputLayoutThemePreference,
                "compact_input_layout"
            ),
            InputLayoutVariant::Expanded => get_theme!(
                &self.theme_layout,
                InputLayoutThemePreference,
                "expanded_input_layout"
            ),
        };

        let (mut movement_timeout, cursor_color) =
            use_cursor_blink(focus() != Focus::Not, theme_colors.color);

        let enabled = use_reactive(&self.enabled);

        let display_placeholder = value.read().is_empty()
            && self.placeholder.is_some()
            && !editable.editor().read().has_preedit();
        let on_validate = self.on_validate.clone();
        let on_submit = self.on_submit.clone();

        if *value.read() != editable.editor().read().committed_text() {
            let mut editor = editable.editor_mut().write();
            editor.clear_preedit();
            editor.set(&value.read());
            editor.editor_history_mut().clear();
            editor.clear_selection();
        }

        let mode = self.mode;
        let text_align = self.text_align;
        let padding = theme_layout.padding;
        let multiline = self.multiline;
        let mut follow_cursor = move || {
            if !a11y_id.is_focused() || display_placeholder {
                return;
            }

            let holder = holder.peek();
            let holder = holder.0.borrow();
            let Some(ParagraphHolderInner {
                paragraph,
                scale_factor,
                ..
            }) = holder.as_ref()
            else {
                warn!("Paragraph should be build by now.");
                return;
            };

            let viewport = viewport_area();
            if viewport.width() == 0. {
                return;
            }

            // Text as currently displayed
            let editor = editable.editor().peek();
            let text = match mode {
                InputMode::Hidden(character) => {
                    character.to_string().repeat(editor.rope().len_chars())
                }
                InputMode::Shown => editor.rope().to_string(),
            };

            let cursor_rect = paragraph.cursor_rect(&text, editor.cursor_pos(), text_align);
            let cursor_location =
                Point2D::new(cursor_rect.left, cursor_rect.top) / (*scale_factor as f32);
            let visible_cursor_location =
                cursor_location + Vector2D::new(padding.left(), padding.top());
            // Visible window start
            let visible_start_x = viewport.min_x() - area.peek().min_x();

            // Minimally reveal the cursor
            if visible_cursor_location.x < visible_start_x {
                scroll_controller.scroll_to_x(-cursor_location.x as i32);
            } else if visible_cursor_location.x > visible_start_x + viewport.width() {
                scroll_controller
                    .scroll_to_x(-(area.peek().width() - viewport.width()).max(0.0) as i32);
            }

            if multiline {
                let cursor_bottom = cursor_rect.bottom / (*scale_factor as f32) + padding.bottom();
                let visible_start_y = viewport.min_y() - area.peek().min_y();

                if visible_cursor_location.y < visible_start_y {
                    scroll_controller.scroll_to_y(-cursor_location.y as i32);
                } else if cursor_bottom > visible_start_y + viewport.height() {
                    scroll_controller.scroll_to_y(-(cursor_bottom - viewport.height()) as i32);
                }
            }
        };

        let probe_state = self.probe;
        let publish_probe = move || {
            let Some(mut probe_state) = probe_state else {
                return;
            };
            let editor = editable.editor().peek();
            let holder = holder.peek();
            let holder = holder.0.borrow();
            let Some(ParagraphHolderInner {
                paragraph,
                scale_factor,
                ..
            }) = holder.as_ref()
            else {
                return;
            };
            let area_now = *area.peek();
            let focused = a11y_id.is_focused();
            let shows_caret = focused && !display_placeholder;
            let text = match mode {
                InputMode::Hidden(character) => {
                    character.to_string().repeat(editor.rope().len_chars())
                }
                InputMode::Shown => editor.rope().to_string(),
            };
            let scale = *scale_factor as f32;
            // The paragraph's area includes its padding; glyphs start inside it.
            let caret_at = |index: usize| {
                let rect = paragraph.cursor_rect(&text, index, text_align);
                InputCaret {
                    x: area_now.min_x() + padding.left() + rect.left / scale,
                    top: area_now.min_y() + padding.top() + rect.top / scale,
                    bottom: area_now.min_y() + padding.top() + rect.bottom / scale,
                }
            };
            let next = InputProbe {
                focused,
                area: area_now,
                caret: shows_caret.then(|| caret_at(editor.cursor_pos())),
                selection: editor
                    .get_selection_range()
                    .filter(|(start, end)| start != end && shows_caret)
                    .map(|(start, end)| (caret_at(start), caret_at(end))),
            };
            probe_state.set_if_modified(next);
        };
        publish_probe();

        let on_ime_preedit = move |e: Event<ImePreeditEventData>| {
            let mut editor = editable.editor_mut().write();
            if e.data().text.is_empty() {
                editor.clear_preedit();
            } else {
                editor.set_preedit(&e.data().text);
            }
        };

        let on_pre_key_down = self.on_pre_key_down.clone().unwrap_or_else(|| {
            Callback::new(move |e: Event<KeyboardEventData>| match &e.key {
                Key::Named(NamedKey::Enter) if !multiline => true,
                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Shift) => true,
                Key::Named(NamedKey::Tab) => false,
                _ => {
                    e.stop_propagation();
                    e.prevent_default();
                    true
                }
            })
        });
        let mut value_for_command = value.clone();
        let on_validate_for_command = on_validate.clone();

        let on_key_down = move |e: Event<KeyboardEventData>| {
            let key = e.key.clone();
            let modifiers = e.modifiers;

            if !on_pre_key_down.call(e) {
                return;
            }

            match &key {
                // On submit
                Key::Named(NamedKey::Enter) if !multiline => {
                    if let Some(on_submit) = &on_submit {
                        let text = editable.editor().peek().committed_text();
                        on_submit.call(text);
                    }
                }
                // On unfocus
                Key::Named(NamedKey::Escape) => {
                    a11y_id.request_unfocus();
                }
                // On change
                _ => {
                    movement_timeout.reset();
                    let changed = apply_key_edit(
                        &mut editable,
                        &mut value,
                        on_validate.as_ref(),
                        &key,
                        modifiers,
                        &holder.read(),
                    );
                    if !changed {
                        follow_cursor();
                    }
                }
            }
        };

        // Runs a command set by the surrounding UI, such as a context menu.
        let command_state = self.command;
        let pending = match self.command {
            Some(command) => *command.read(),
            None => None,
        };
        use_side_effect_with_deps(&pending, move |pending| {
            let Some(command) = *pending else {
                return;
            };
            if let Some(mut state) = command_state {
                state.set(None);
            }
            // The edit runs in its own task: reads in it must not subscribe this
            // effect, or the writes it makes would rerun the effect for ever.
            let mut value_for_command = value_for_command.clone();
            let on_validate_for_command = on_validate_for_command.clone();
            spawn(async move {
                let mut editable = editable;
                let character = match command {
                    InputCommand::SelectAll => "a",
                    InputCommand::Copy => "c",
                    InputCommand::Cut => "x",
                    InputCommand::Paste => "v",
                };
                let changed = apply_key_edit(
                    &mut editable,
                    &mut value_for_command,
                    on_validate_for_command.as_ref(),
                    &Key::Character(character.to_string()),
                    Modifiers::ctrl_or_meta(),
                    &holder.peek(),
                );
                a11y_id.request_focus();
                if !changed {
                    follow_cursor();
                }
            });
        });

        // Runs a request set by the surrounding UI, such as selecting the word under a touch.
        let request_state = self.request;
        let pending_request = match self.request {
            Some(request) => *request.read(),
            None => None,
        };
        use_side_effect_with_deps(&pending_request, move |pending| {
            let Some(request) = *pending else {
                return;
            };
            if let Some(mut state) = request_state {
                state.set(None);
            }
            // The edit runs in its own task: reads in it must not subscribe this
            // effect, or the writes it makes would rerun the effect for ever.
            spawn(async move {
                a11y_id.request_focus();
                let mut editable = editable;
                match request {
                    InputRequest::SelectWordAt { x, y } => {
                        if !display_placeholder {
                            let text_area = area.peek().to_f64();
                            let global_location = CursorPoint::new(x as f64, y as f64)
                                .clamp(text_area.min(), text_area.max());
                            let location = (global_location - text_area.min()).to_point();
                            editable.process_event(EditableEvent::Down {
                                location,
                                editor_line: EditorLine::SingleParagraph,
                                holder: &holder.peek(),
                            });
                            let pos = editable.editor().peek().cursor_pos();
                            let (start, end) = editable.editor().peek().find_word_boundaries(pos);
                            editable.editor_mut().write().set_selection((start, end));
                            follow_cursor();
                        }
                    }
                    InputRequest::SelectWordAtCursor => {
                        if !display_placeholder {
                            let editor = editable.editor().peek();
                            let pos = editor.cursor_pos();
                            let mut word = editor.find_word_boundaries(pos);
                            // A caret at the end of a word has no word after it: take the one before.
                            if word.0 == word.1 && pos > 0 {
                                word = editor.find_word_boundaries(pos - 1);
                            }
                            drop(editor);
                            editable.editor_mut().write().set_selection(word);
                            follow_cursor();
                        }
                    }
                    InputRequest::DragSelectionEnd { start } => {
                        let range = editable.editor().peek().get_selection_range();
                        if let Some((from, to)) = range {
                            // The end that is not dragged stays the anchor; the dragged end becomes the cursor.
                            let selection = if start { (to, from) } else { (from, to) };
                            editable.editor_mut().write().set_selection(selection);
                        }
                        is_dragging.set(true);
                    }
                }
            });
        });

        let on_key_up = move |e: Event<KeyboardEventData>| {
            e.stop_propagation();
            editable.process_event(EditableEvent::KeyUp { key: &e.key });
        };

        let on_input_focus_press = move |e: Event<FocusPressEventData>| {
            // A touch stays unstopped and unprevented so the field's global pointer handlers hear it;
            // Freya cancels global pointer events once a press is stopped or prevented.
            if !matches!(e.data(), FocusPressEventData::Touch(_)) {
                e.stop_propagation();
                e.prevent_default();
            }
            if cfg!(target_os = "android") {
                if a11y_id.is_focused() {
                    // Require a second press to enabling dragging on Android
                    is_dragging.set_if_modified(true);
                }
            } else {
                is_dragging.set_if_modified(true);
            }
            movement_timeout.reset();
            if !display_placeholder {
                let text_area = area.read().to_f64();
                let global_location = e.global_location().clamp(text_area.min(), text_area.max());
                let location = (global_location - text_area.min()).to_point();
                editable.process_event(EditableEvent::Down {
                    location,
                    editor_line: EditorLine::SingleParagraph,
                    holder: &holder.read(),
                });
            }
            a11y_id.request_focus();
        };

        let on_focus_press = move |e: Event<FocusPressEventData>| {
            if !matches!(e.data(), FocusPressEventData::Touch(_)) {
                e.stop_propagation();
                e.prevent_default();
            }
            if cfg!(target_os = "android") {
                if a11y_id.is_focused() {
                    // Require a second press to enabling dragging on Android
                    is_dragging.set_if_modified(true);
                }
            } else {
                is_dragging.set_if_modified(true);
            }
            movement_timeout.reset();
            if !display_placeholder {
                editable.process_event(EditableEvent::Down {
                    location: e.element_location(),
                    editor_line: EditorLine::SingleParagraph,
                    holder: &holder.read(),
                });
            }
            a11y_id.request_focus();
        };

        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            if a11y_id.is_focused() && *is_dragging.read() {
                let text_area = area.read().to_f64();
                let location = (e.global_location() - text_area.min()).to_point();
                editable.process_event(EditableEvent::Move {
                    location,
                    editor_line: EditorLine::SingleParagraph,
                    holder: &holder.read(),
                });
                follow_cursor();
            }
        };

        let on_pointer_enter = move |_| {
            *status.write() = InputStatus::Hovering;
        };

        let on_pointer_leave = move |_| {
            if status() == InputStatus::Hovering {
                *status.write() = InputStatus::default();
            }
        };

        let companions = self.keep_focus_in.clone();
        let on_global_pointer_up = move |e: Event<PointerEventData>| {
            match *status.read() {
                InputStatus::Idle if a11y_id.is_focused() => {
                    editable.process_event(EditableEvent::Release);
                }
                InputStatus::Hovering => {
                    editable.process_event(EditableEvent::Release);
                }
                _ => {}
            };

            // A touch press reaches the input after its release, so a tap on a field that is
            // already focused finds it not dragging. The tap is inside it, not outside.
            let tapped_inside = matches!(e.data(), PointerEventData::Touch(_))
                && (outer_area.read().to_f64().contains(e.global_location())
                    || companions
                        .iter()
                        .any(|area| area.to_f64().contains(e.global_location())));
            if a11y_id.is_focused() {
                if *is_dragging.read() {
                    // The input is focused and dragging, but it just clicked so we assume the dragging can stop
                    is_dragging.set(false);
                } else if !tapped_inside {
                    // The input is focused but not dragging, so the click means it was clicked outside, therefore we can unfocus this input
                    a11y_id.request_unfocus();
                }
            }
        };

        let on_pointer_press = move |e: Event<PointerEventData>| {
            if !matches!(e.data(), PointerEventData::Touch(_)) {
                e.stop_propagation();
                e.prevent_default();
            }
            match *status.read() {
                InputStatus::Idle if a11y_id.is_focused() => {
                    editable.process_event(EditableEvent::Release);
                }
                InputStatus::Hovering => {
                    editable.process_event(EditableEvent::Release);
                }
                _ => {}
            };

            if a11y_id.is_focused() {
                is_dragging.set_if_modified(false);
            }
        };

        let on_paragraph_sized = move |e: Event<SizedEventData>| {
            let viewport = viewport_area();
            if viewport.width() > 0. {
                scroll_controller.apply_layout(e.area.size, viewport.size);
            }

            let text_size_changed = area.peek().size != e.area.size;
            area.set_if_modified(e.area);
            if text_size_changed {
                follow_cursor();
            }
            publish_probe();
        };

        let (background, cursor_index, text_selection) = if enabled() && focus() != Focus::Not {
            (
                theme_colors.focus_background,
                Some(editable.editor().read().cursor_pos()),
                editable
                    .editor()
                    .read()
                    .get_visible_selection(EditorLine::SingleParagraph),
            )
        } else if enabled() && self.keep_selection {
            // Focus sits elsewhere, such as the field's own menu; the selection stays shown.
            (
                theme_colors.background,
                None,
                editable
                    .editor()
                    .read()
                    .get_visible_selection(EditorLine::SingleParagraph),
            )
        } else {
            (theme_colors.background, None, None)
        };

        let border = if focus().is_focused() {
            Border::new()
                .fill(theme_colors.focus_border_fill)
                .width(2.)
                .alignment(BorderAlignment::Inner)
        } else {
            Border::new()
                .fill(theme_colors.border_fill.mul_if(!self.enabled, 0.85))
                .width(1.)
                .alignment(BorderAlignment::Inner)
        };

        let color = if display_placeholder {
            theme_colors.placeholder_color
        } else {
            theme_colors.color
        };

        let value = self.value.read();
        let a11y_text: Cow<str> = match (self.mode, &self.placeholder) {
            (_, Some(ph)) if display_placeholder => Cow::Borrowed(ph.as_ref()),
            (InputMode::Hidden(ch), _) => Cow::Owned(ch.to_string().repeat(value.len())),
            (InputMode::Shown, _) => Cow::Borrowed(value.as_ref()),
        };

        let a11_role = match self.mode {
            InputMode::Hidden(_) => AccessibilityRole::PasswordInput,
            _ if self.multiline => AccessibilityRole::MultilineTextInput,
            _ => AccessibilityRole::TextInput,
        };

        rect()
            .a11y_id(a11y_id)
            .on_sized(move |e: Event<SizedEventData>| outer_area.set_if_modified(e.area))
            .a11y_enabled(self.enabled)
            .a11y_focusable(self.enabled)
            .a11y_auto_focus(self.auto_focus)
            .a11y_alt(a11y_text)
            .a11y_role(a11_role)
            .maybe(self.enabled, |el| {
                el.on_key_up(on_key_up)
                    .on_key_down(on_key_down)
                    .on_focus_press(on_input_focus_press)
                    .on_ime_preedit(on_ime_preedit)
                    .on_pointer_press(on_pointer_press)
                    .on_global_pointer_up(on_global_pointer_up)
                    .on_global_pointer_move(on_global_pointer_move)
            })
            .on_pointer_enter(on_pointer_enter)
            .on_pointer_leave(on_pointer_leave)
            .cursor(if self.enabled {
                CursorIcon::Text
            } else {
                CursorIcon::NotAllowed
            })
            .width(self.width.clone())
            .height(self.height.clone())
            .background(background.mul_if(!self.enabled, 0.85))
            .border(border)
            .corner_radius(theme_layout.corner_radius)
            .content(Content::Flex)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .maybe_child(
                self.leading
                    .clone()
                    .map(|leading| rect().padding(Gaps::new(0., 0., 0., 8.)).child(leading)),
            )
            .child(
                ScrollView::new_controlled(scroll_controller)
                    .width(Size::flex(1.))
                    .height(if self.multiline && !matches!(self.height, Size::Inner) {
                        Size::fill()
                    } else {
                        Size::Inner
                    })
                    .direction(if self.multiline {
                        Direction::Vertical
                    } else {
                        Direction::Horizontal
                    })
                    .show_scrollbar(self.multiline)
                    .on_sized(move |e: Event<SizedEventData>| viewport_area.set_if_modified(e.area))
                    .child(
                        paragraph()
                            .holder(holder.read().clone())
                            .on_sized(on_paragraph_sized)
                            .min_width(Size::percent(100.))
                            .maybe(self.multiline, |el| el.max_width(Size::percent(100.)))
                            .maybe(self.enabled, |el| el.on_focus_press(on_focus_press))
                            .padding(theme_layout.padding)
                            .cursor_index(cursor_index)
                            .cursor_color(cursor_color)
                            .color(color)
                            .text_align(self.text_align)
                            .map(self.font_family, |el, family| el.font_family(family))
                            .maybe(!self.multiline, |el| el.max_lines(1))
                            .highlights(text_selection.map(|h| vec![h]))
                            .maybe(display_placeholder, |el| {
                                el.span(self.placeholder.as_ref().unwrap().to_string())
                            })
                            .maybe(!display_placeholder, |el| {
                                let editor = editable.editor().read();
                                if editor.has_preedit() {
                                    let (b, p, a) = editor.preedit_text_segments();
                                    let (b, p, a) = match self.mode {
                                        InputMode::Hidden(ch) => {
                                            let ch = ch.to_string();
                                            (
                                                ch.repeat(b.chars().count()),
                                                ch.repeat(p.chars().count()),
                                                ch.repeat(a.chars().count()),
                                            )
                                        }
                                        InputMode::Shown => (b, p, a),
                                    };
                                    el.span(b)
                                        .span(
                                            Span::new(p).text_decoration(TextDecoration::Underline),
                                        )
                                        .span(a)
                                } else {
                                    let text = match self.mode {
                                        InputMode::Hidden(ch) => {
                                            ch.to_string().repeat(editor.rope().len_chars())
                                        }
                                        InputMode::Shown => editor.rope().to_string(),
                                    };
                                    el.span(text)
                                }
                            }),
                    ),
            )
            .maybe_child(
                self.trailing
                    .clone()
                    .map(|trailing| rect().padding(Gaps::new(0., 8., 0., 0.)).child(trailing)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
