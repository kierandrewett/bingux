//! Users: the accounts AccountsService knows about, read with `busctl` on the
//! system bus and listed with the current user first. Pressing a row opens
//! its details, where the full name is written with `SetRealName` (polkit may
//! ask for authentication) and the password is changed in a terminal running
//! `passwd`.

use std::process::Command;
use std::time::Duration;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::routes::{Route, use_navigator};
use crate::system::{self, CommandError, CommandResult, background, background_then};
use crate::theme::Tokens;
use crate::ui::{
    Group, Status, caption, page_column, row, skeleton, status, text_row, use_overlays, use_tokens,
};
use bingux_ui::scale::*;

const ACCOUNTS: &str = "org.freedesktop.Accounts";
const ACCOUNTS_PATH: &str = "/org/freedesktop/Accounts";
const USER_INTERFACE: &str = "org.freedesktop.Accounts.User";
/// `AccountType` value for an administrator; every other value is standard.
const ACCOUNT_TYPE_ADMINISTRATOR: f64 = 1.0;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Writes can wait for a polkit authentication dialog.
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// Terminal launchers that can run `passwd`, in order of preference.
const PASSWORD_TERMINALS: [(&str, &[&str]); 3] = [
    ("gnome-terminal", &["--", "passwd"]),
    ("kgx", &["--", "passwd"]),
    ("xdg-terminal-exec", &["passwd"]),
];

#[derive(Clone, PartialEq)]
struct Account {
    path: String,
    user_name: String,
    real_name: String,
    administrator: bool,
    locked: bool,
    automatic_login: bool,
}

impl Account {
    /// The full name when one is set, otherwise the user name.
    fn display_name(&self) -> &str {
        if self.real_name.is_empty() {
            &self.user_name
        } else {
            &self.real_name
        }
    }
}

/// Every cached account, the current user first. Runs on a background thread.
fn load() -> Result<Vec<Account>, CommandError> {
    let current = system::run("id", &["-un"], READ_TIMEOUT)?;
    let listed = busctl_json(
        &["call", ACCOUNTS, ACCOUNTS_PATH, ACCOUNTS, "ListCachedUsers"],
        READ_TIMEOUT,
    )?;
    // `ao` reply: one output argument holding an array of object paths.
    let paths = listed
        .field("data")
        .and_then(Json::as_array)
        .and_then(|arguments| arguments.first())
        .and_then(Json::as_array)
        .ok_or_else(unexpected)?;
    let mut accounts = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.as_str().ok_or_else(unexpected)?;
        accounts.push(read_account(path)?);
    }
    accounts.sort_by_key(|account| account.user_name != current);
    Ok(accounts)
}

fn read_account(path: &str) -> Result<Account, CommandError> {
    Ok(Account {
        path: path.to_string(),
        user_name: text_property(path, "UserName")?,
        real_name: text_property(path, "RealName")?,
        administrator: number_property(path, "AccountType")? == ACCOUNT_TYPE_ADMINISTRATOR,
        locked: bool_property(path, "Locked")?,
        automatic_login: bool_property(path, "AutomaticLogin")?,
    })
}

fn busctl_json(arguments: &[&str], timeout: Duration) -> Result<Json, CommandError> {
    let mut full = vec!["--system", "--json=short"];
    full.extend_from_slice(arguments);
    let text = system::run("busctl", &full, timeout)?;
    Json::parse(&text).map_err(|_| unexpected())
}

/// The `data` of `busctl get-property` for one `org.freedesktop.Accounts.User`
/// property.
fn property(path: &str, name: &str) -> Result<Json, CommandError> {
    busctl_json(
        &["get-property", ACCOUNTS, path, USER_INTERFACE, name],
        READ_TIMEOUT,
    )?
    .field("data")
    .cloned()
    .ok_or_else(unexpected)
}

fn text_property(path: &str, name: &str) -> Result<String, CommandError> {
    property(path, name)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(unexpected)
}

fn number_property(path: &str, name: &str) -> Result<f64, CommandError> {
    property(path, name)?.as_number().ok_or_else(unexpected)
}

fn bool_property(path: &str, name: &str) -> Result<bool, CommandError> {
    property(path, name)?.as_bool().ok_or_else(unexpected)
}

fn unexpected() -> CommandError {
    CommandError("Account details could not be read.".into())
}

/// Set the full name of the account at `path` through polkit.
fn set_real_name(path: &str, name: &str) -> CommandResult {
    system::run(
        "busctl",
        &[
            "--system",
            "call",
            ACCOUNTS,
            path,
            USER_INTERFACE,
            "SetRealName",
            "s",
            name,
        ],
        WRITE_TIMEOUT,
    )
}

/// Open the first installed terminal running `passwd`. The launcher returns
/// once the terminal is up, so waiting on it only reaps the process.
fn change_password() -> Result<(), CommandError> {
    let (program, arguments) = PASSWORD_TERMINALS
        .iter()
        .find(|(program, _)| system::which(program))
        .ok_or_else(|| {
            CommandError("No terminal app was found. Install one, then try again.".into())
        })?;
    let mut child = Command::new(program)
        .args(*arguments)
        .spawn()
        .map_err(|_| CommandError("The terminal window could not be opened.".into()))?;
    child
        .wait()
        .map_err(|_| CommandError("The terminal window closed unexpectedly.".into()))?;
    Ok(())
}

/// Up to two capital letters: the first letter of the first and last words.
/// Up to two initials for an avatar, from the first and last word.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let letters: String = match words.as_slice() {
        [] => String::new(),
        [only] => only.chars().take(1).collect(),
        [first, .., last] => first.chars().take(1).chain(last.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

#[derive(PartialEq)]
pub struct UsersPage;

impl Component for UsersPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("users", load);
        let navigator = use_navigator();

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let accounts = match loaded {
            Ok(accounts) => accounts,
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(model, load)),
                );
            }
        };

        let group = Group::titled("Accounts");
        let group = if accounts.is_empty() {
            group.row(status(Status::Empty("No accounts found.".into())))
        } else {
            group.rows(accounts.iter().map(|account| {
                let route = Route::Account {
                    path: account.path.clone(),
                    name: account.real_name.clone(),
                };
                let on_press: EventHandler<()> =
                    (move |_: ()| navigator.push(route.clone())).into();
                Element::from(AccountRow {
                    account: account.clone(),
                    on_press,
                })
            }))
        };
        page_column().child(group)
    }
}

/// One account: its full name, user name, type, and the password.
#[derive(PartialEq)]
pub struct AccountPage {
    pub path: String,
}

impl Component for AccountPage {
    fn render(&self) -> impl IntoElement {
        let model: State<Option<Result<Vec<Account>, CommandError>>> =
            system::use_cached("users", load);
        let overlays = use_overlays();
        let tokens: Tokens = use_tokens();
        let Some(Ok(accounts)) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let Some(account) = accounts.iter().find(|account| account.path == self.path) else {
            return page_column().child(status(Status::Empty(
                "This account no longer exists.".to_string(),
            )));
        };
        let draft = use_state(|| account.real_name.clone());
        let path = account.path.clone();
        let name_row = text_row("Full name", draft, move |name: String| {
            let path = path.clone();
            background_then(
                move || set_real_name(&path, &name),
                move |result: CommandResult| match result {
                    Ok(_) => {
                        background(model, load);
                    }
                    Err(e) => {
                        overlays.toast(e.0);
                    }
                },
            );
        });
        let password_row = row("Change password").navigates().on_press(move |_| {
            background_then(change_password, move |result: Result<(), CommandError>| {
                if let Err(e) = result {
                    overlays.toast(e.0);
                }
            });
        });
        let role = if account.administrator {
            "Administrator"
        } else {
            "Standard"
        };
        let avatar = rect()
            .width(Size::px(BOX_2XL))
            .height(Size::px(BOX_2XL))
            .corner_radius(BOX_2XL / 2.0)
            .background(tokens.accent)
            .center()
            .child(
                label()
                    .text(initials(account.display_name()))
                    .color(tokens.accent_foreground)
                    .font_size(TEXT_DISPLAY)
                    .font_weight(FontWeight::SEMI_BOLD),
            );
        let header = rect()
            .width(Size::fill())
            .spacing(S1)
            .cross_align(Alignment::Center)
            .child(avatar)
            .child(
                label()
                    .text(account.display_name().to_string())
                    .color(tokens.text)
                    .font_size(TEXT_XL)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(caption(role));
        page_column()
            .child(header)
            .child(
                Group::titled("Account")
                    .row(name_row)
                    .row(row("User name").value(account.user_name.clone()))
                    .row(row("Account type").value(if account.administrator {
                        "Administrator"
                    } else {
                        "Standard"
                    }))
                    .row(row("Automatic login").value(if account.automatic_login {
                        "Enabled"
                    } else {
                        "Disabled"
                    }))
                    .row(row("Account locked").value(if account.locked { "Yes" } else { "No" })),
            )
            .child(
                Group::titled("Password")
                    .description("Opens a terminal window where you type your new password.")
                    .row(password_row),
            )
    }
}

/// One account in the list: initials avatar, full name with the user name
/// underneath, the account type, and a chevron. The whole row opens details.
#[derive(PartialEq)]
struct AccountRow {
    account: Account,
    on_press: EventHandler<()>,
}

impl Component for AccountRow {
    fn render(&self) -> impl IntoElement {
        let tokens: Tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let on_press = self.on_press.clone();
        let name = self.account.display_name().to_string();
        let role = if self.account.administrator {
            "Administrator"
        } else {
            "Standard"
        };
        let subtitle = (!self.account.real_name.is_empty()).then(|| {
            label()
                .text(self.account.user_name.clone())
                .color(tokens.muted)
                .font_size(TEXT_MD)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
        });

        let avatar = rect()
            .width(Size::px(BOX_LG))
            .height(Size::px(BOX_LG))
            .corner_radius(BOX_LG / 2.0)
            .background(tokens.accent)
            .center()
            .child(
                label()
                    .text(initials(&name))
                    .color(tokens.accent_foreground)
                    .font_size(TEXT_MD)
                    .font_weight(FontWeight::SEMI_BOLD),
            );
        let text_column = rect()
            .width(Size::flex(1.0))
            .main_align(Alignment::Center)
            .spacing(S0)
            .child(
                label()
                    .text(name)
                    .color(tokens.text)
                    .font_size(TEXT_LG)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(subtitle);

        rect()
            .width(Size::fill())
            .height(Size::px(TALL_ROW_HEIGHT))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((0.0, GUTTER))
            .spacing(S3)
            .background(if hovering() {
                tokens.hover
            } else {
                Color::TRANSPARENT
            })
            .a11y_role(AccessibilityRole::Button)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_: Event<PressEventData>| on_press.call(()))
            .child(avatar)
            .child(text_column)
            .child(
                label()
                    .text(role)
                    .color(tokens.muted)
                    .font_size(TEXT_MD)
                    .max_lines(1),
            )
            .child(icons::icon(Icon::GoNext, ICON_MD, tokens.muted))
    }
}

/// The subset of JSON that `busctl --json=short` prints. The crate has no
/// JSON dependency, so this small parser reads the Accounts replies.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn parse(text: &str) -> Result<Self, String> {
        let mut parser = Parser { rest: text };
        let value = parser.value()?;
        parser.skip_space();
        if parser.rest.is_empty() {
            Ok(value)
        } else {
            Err(format!("unexpected text after the value: {}", parser.rest))
        }
    }

    fn field(&self, name: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(text),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn as_number(&self) -> Option<f64> {
        match self {
            Json::Number(value) => Some(*value),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }
}

struct Parser<'a> {
    rest: &'a str,
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        self.rest = self.rest.trim_start();
    }

    fn eat(&mut self, expected: char) -> bool {
        match self.rest.strip_prefix(expected) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.eat(expected) {
            Ok(())
        } else {
            Err(format!("expected '{expected}'"))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_space();
        match self.rest.chars().next() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => self.string().map(Json::Str),
            Some('t') => self.word("true", Json::Bool(true)),
            Some('f') => self.word("false", Json::Bool(false)),
            Some('n') => self.word("null", Json::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected '{c}'")),
            None => Err("unexpected end of text".into()),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect('{')?;
        let mut fields = Vec::new();
        self.skip_space();
        if !self.eat('}') {
            loop {
                self.skip_space();
                let key = self.string()?;
                self.skip_space();
                self.expect(':')?;
                let value = self.value()?;
                fields.push((key, value));
                self.skip_space();
                if !self.eat(',') {
                    break;
                }
            }
            self.expect('}')?;
        }
        Ok(Json::Object(fields))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_space();
        if !self.eat(']') {
            loop {
                items.push(self.value()?);
                self.skip_space();
                if !self.eat(',') {
                    break;
                }
            }
            self.expect(']')?;
        }
        Ok(Json::Array(items))
    }

    fn string(&mut self) -> Result<String, String> {
        let body = self.rest.strip_prefix('"').ok_or("expected a string")?;
        let mut chars = body.chars();
        let mut out = String::new();
        loop {
            match chars.next().ok_or("unterminated string")? {
                '"' => break,
                '\\' => out.push(match chars.next().ok_or("unterminated escape")? {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        u32::from_str_radix(&hex, 16)
                            .ok()
                            .and_then(char::from_u32)
                            .ok_or("unsupported unicode escape")?
                    }
                    other => return Err(format!("unsupported escape '\\{other}'")),
                }),
                c => out.push(c),
            }
        }
        self.rest = chars.as_str();
        Ok(out)
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        self.rest = self
            .rest
            .strip_prefix(word)
            .ok_or_else(|| format!("expected '{word}'"))?;
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, String> {
        let end = self
            .rest
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .unwrap_or(self.rest.len());
        let (text, rest) = self.rest.split_at(end);
        let number = text
            .parse::<f64>()
            .map_err(|_| format!("bad number '{text}'"))?;
        self.rest = rest;
        Ok(Json::Number(number))
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("users", load);
}
