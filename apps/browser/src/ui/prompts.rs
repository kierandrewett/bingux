//! What a page asks of the person: a permission (a banner under the toolbar,
//! never in the way of the page), and the questions that stop the page until
//! they are answered (a dialog, a password, a list, a file).
//!
//! The first prompt of the tab on show is the one on screen. Prompts of other
//! tabs wait, and the tab says so in the strip.

use bingux_ui::files::choose;
use bingux_ui::prelude::*;
use bingux_ui::web_view::requests::{DialogKind, DialogRequest, PickerMode, PickerRequest};
use freya::prelude::*;

use crate::address::Address;
use crate::model::tab::Prompt;

use super::ctx::Ctx;

/// Shows the dialog of the first prompt of the tab on show, once for each.
/// It draws nothing of its own.
#[derive(PartialEq)]
pub struct PromptHost {
    pub ctx: Ctx,
}

/// What names a prompt so that a new one is a new dialog.
fn prompt_id(ctx: Ctx) -> Option<(u64, u32)> {
    let browser = ctx.browser.read();
    let key = browser.tabs.active_key();
    browser.prompt(key).and_then(|prompt| match prompt {
        Prompt::Permission(..) => None,
        other => Some((key, other.id().0)),
    })
}

impl Component for PromptHost {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let id = prompt_id(ctx);
        use_side_effect_with_deps(&id, move |id: &Option<(u64, u32)>| {
            let Some((key, _)) = *id else {
                return;
            };
            let prompt = ctx.browser.peek().prompt(key).cloned();
            match prompt {
                Some(Prompt::Dialog(_, request)) => show_dialog(ctx, key, &request),
                Some(Prompt::Auth(_, request)) => show_auth(ctx, key, &request),
                Some(Prompt::Select(_, request)) => show_select(ctx, key, &request),
                Some(Prompt::FilePicker(_, request)) => show_picker(ctx, key, &request),
                _ => {}
            }
        });
        rect().width(Size::px(0.0)).height(Size::px(0.0))
    }
}

fn field(state: State<String>) -> Element {
    text_field(state, |_: String| {})
        .fill_width()
        .autofocus()
        .into()
}

fn show_dialog(ctx: Ctx, key: u64, request: &DialogRequest) {
    let host = Address::new(&request.origin).host();
    let title = match &request.kind {
        DialogKind::BeforeUnload => "Leave this page?".to_string(),
        _ if host.is_empty() => "This page says".to_string(),
        _ => format!("{host} says"),
    };
    let answer = move |accept: bool| {
        move |_: ()| {
            let text = ctx.chrome.dialog_text.peek().clone();
            ctx.with(|browser| browser.answer_dialog(key, accept, accept.then_some(text)));
        }
    };
    let message = if request.kind == DialogKind::BeforeUnload && request.message.is_empty() {
        "Changes you made may not be saved.".to_string()
    } else {
        request.message.clone()
    };
    let shown = match &request.kind {
        DialogKind::Alert => dialog(title, message)
            .response("OK", ResponseKind::Suggested, answer(true))
            .default_action("ok")
            .cancel_action("ok"),
        DialogKind::Confirm => dialog(title, message)
            .action(CANCEL_ID, "Cancel", ResponseKind::Default, answer(false))
            .response("OK", ResponseKind::Suggested, answer(true))
            .default_action("ok"),
        DialogKind::Prompt { default } => {
            let mut text = ctx.chrome.dialog_text;
            text.set(default.clone());
            dialog(title, message)
                .content(field(ctx.chrome.dialog_text))
                .action(CANCEL_ID, "Cancel", ResponseKind::Default, answer(false))
                .response("OK", ResponseKind::Suggested, answer(true))
                .default_action("ok")
        }
        DialogKind::BeforeUnload => dialog(title, message)
            .action(
                CANCEL_ID,
                "Stay on page",
                ResponseKind::Default,
                answer(false),
            )
            .response("Leave", ResponseKind::Destructive, answer(true))
            .default_action(CANCEL_ID),
    };
    ctx.overlays
        .show_dialog(shown.cancel_action(if request.kind == DialogKind::Alert {
            "ok"
        } else {
            CANCEL_ID
        }));
}

fn show_auth(ctx: Ctx, key: u64, request: &bingux_ui::web_view::requests::AuthRequest) {
    let mut name = ctx.chrome.dialog_text;
    let mut password = ctx.chrome.dialog_password;
    name.set(String::new());
    password.set(String::new());
    let who = if request.proxy {
        format!("The proxy {} asks you to sign in", request.host)
    } else {
        format!("Sign in to {}", request.host)
    };
    let body = match (request.realm.is_empty(), request.retry) {
        (false, true) => format!(
            "{} \u{2014} the name or password was not accepted.",
            request.realm
        ),
        (false, false) => request.realm.clone(),
        (true, true) => "The name or password was not accepted.".to_string(),
        (true, false) => "The site asks who you are.".to_string(),
    };
    let form = rect()
        .tw_layout(&[flex_col(), w_full(), gap(S2)])
        .child(
            text_field(ctx.chrome.dialog_text, |_: String| {})
                .fill_width()
                .autofocus()
                .placeholder("Name"),
        )
        .child(
            text_field(ctx.chrome.dialog_password, |_: String| {})
                .fill_width()
                .password()
                .placeholder("Password"),
        );
    ctx.overlays.show_dialog(
        dialog(who, body)
            .content(form)
            .action(CANCEL_ID, "Cancel", ResponseKind::Default, move |_: ()| {
                ctx.with(|browser| browser.answer_auth(key, None));
            })
            .response("Sign in", ResponseKind::Suggested, move |_: ()| {
                let credentials = (
                    ctx.chrome.dialog_text.peek().clone(),
                    ctx.chrome.dialog_password.peek().clone(),
                );
                ctx.with(|browser| browser.answer_auth(key, Some(credentials)));
            })
            .default_action("sign_in")
            .cancel_action(CANCEL_ID),
    );
}

/// A `<select>` list, as a dialog with the options in a dropdown.
fn show_select(ctx: Ctx, key: u64, request: &bingux_ui::web_view::requests::SelectRequest) {
    let options: Vec<(String, String)> = request
        .options
        .iter()
        .enumerate()
        .map(|(at, option)| (at.to_string(), option.label.clone()))
        .collect();
    let first = request
        .options
        .iter()
        .position(|option| option.selected)
        .unwrap_or(0);
    let mut chosen = ctx.chrome.dialog_text;
    chosen.set(first.to_string());
    ctx.overlays.show_dialog(
        dialog("Choose an option", "")
            .content(Element::from(SelectBody { ctx, options }))
            .action(CANCEL_ID, "Cancel", ResponseKind::Default, move |_: ()| {
                ctx.with(|browser| browser.answer_select(key, None));
            })
            .response("Choose", ResponseKind::Suggested, move |_: ()| {
                let index = ctx.chrome.dialog_text.peek().parse::<usize>().ok();
                ctx.with(|browser| browser.answer_select(key, index));
            })
            .default_action("choose")
            .cancel_action(CANCEL_ID),
    );
}

/// The dropdown of a `<select>` dialog. It is a component of its own: it holds
/// a control, and a control has hooks.
#[derive(PartialEq)]
struct SelectBody {
    ctx: Ctx,
    options: Vec<(String, String)>,
}

impl Component for SelectBody {
    fn render(&self) -> impl IntoElement {
        let mut chosen = self.ctx.chrome.dialog_text;
        dropdown(
            self.options.clone(),
            chosen.read().clone(),
            move |value: String| chosen.set(value),
        )
    }
}

/// `<input type=file>`, through the file chooser every Bingux app has.
fn show_picker(ctx: Ctx, key: u64, request: &PickerRequest) {
    let chooser = match request.mode {
        PickerMode::Open => ChooserRequest::open().title("Choose a file"),
        PickerMode::OpenMany => ChooserRequest::open().multiple(true).title("Choose files"),
        PickerMode::Folder => ChooserRequest::select_folder(),
        PickerMode::Save => {
            ChooserRequest::save(request.suggested_name.clone().unwrap_or_default())
        }
    };
    choose(ctx.window, chooser, move |chosen| {
        let paths = chosen.map(|chosen| chosen.paths).unwrap_or_default();
        ctx.with(|browser| browser.answer_picker(key, paths));
    });
}
