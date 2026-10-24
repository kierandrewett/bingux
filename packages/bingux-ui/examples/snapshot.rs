//! Renders every gallery page headlessly to `/tmp/bingux-ui-gallery/`.
//!
//! Run with `cargo run --example snapshot --features testing`.

#[path = "gallery/pages.rs"]
mod pages;

use bingux_ui::controls::text_field::text_field;
use bingux_ui::prelude::{
    OverlayHost, PopupAction, PopupActions, WindowHost, popup, use_popup_actions, use_window,
};
use bingux_ui::snapshot::{Snapshot, snapshot};
use bingux_ui::style::{Paint, S3, Tw, bg, flex_col, gap};
use bingux_ui::surface::use_tokens;
use bingux_ui::theme::WINDOW_SIZE;
use freya::prelude::*;
use pages::{Gallery, Page};

fn main() {
    let size = match std::env::var("SNAPSHOT_HEIGHT") {
        Ok(height) => (WINDOW_SIZE.0, height.parse().unwrap_or(WINDOW_SIZE.1)),
        Err(_) => WINDOW_SIZE,
    };
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/bingux-ui-gallery".to_string());
    let tokens = bingux_ui::theme::load_tokens();
    for page in Page::ALL {
        let name = format!("{page:?}").to_lowercase();
        let path = format!("{out}/{name}.png");
        snapshot(
            move || {
                Element::from(
                    rect()
                        .expanded()
                        .tw(&tokens, &[bg(Paint::Bg)])
                        .child(Gallery { initial: page }),
                )
            },
            size,
            700,
            &path,
        );
        println!("[snapshot] wrote {path}");
    }

    // Interactive states: the dropdown open and an option hovered. The
    // frame is taller so the Choice group is in view.
    let path = format!("{out}/controls-dropdown.png");
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Controls,
                    }),
            )
        },
        (WINDOW_SIZE.0, 1100.0),
    );
    shot.settle(std::time::Duration::from_millis(400))
        .click(823.0, 943.0)
        .settle(std::time::Duration::from_millis(300))
        .hover(780.0, 870.0)
        .settle(std::time::Duration::from_millis(100))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // A switch row under the pointer: the row lights and its hairlines go.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Controls,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/controls-hover.png");
    shot.settle(std::time::Duration::from_millis(400))
        .hover(600.0, 192.0)
        .settle(std::time::Duration::from_millis(200))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // A subpage opened from a navigating row, with the header's Back button.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Rows,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/rows-subpage.png");
    shot.settle(std::time::Duration::from_millis(400))
        .click(580.0, 393.0)
        .settle(std::time::Duration::from_millis(400))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // The alert dialog open, then a toast after it is dismissed.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Feedback,
                    }),
            )
        },
        (WINDOW_SIZE.0, 1100.0),
    );
    let path = format!("{out}/feedback-dialog.png");
    shot.settle(std::time::Duration::from_millis(400))
        .click(906.0, 858.0)
        .settle(std::time::Duration::from_millis(400))
        .save(&path);
    println!("[snapshot] wrote {path}");
    let path = format!("{out}/feedback-toast.png");
    shot.click(60.0, 500.0)
        .settle(std::time::Duration::from_millis(300))
        .click(906.0, 914.0)
        .settle(std::time::Duration::from_millis(500))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // The sheet open on mount with one field: it should size to its content.
    let path = format!("{out}/feedback-sheet.png");
    Snapshot::new(
        move || {
            Element::from(Themed {
                child: WindowHost::new(OverlayHost::new(SheetOpen { long: false })).into(),
            })
        },
        size,
    )
    .settle(std::time::Duration::from_millis(600))
    .save(&path);
    println!("[snapshot] wrote {path}");

    // The same sheet with a body taller than the cap: the body should scroll.
    let path = format!("{out}/feedback-sheet-long.png");
    Snapshot::new(
        move || {
            Element::from(Themed {
                child: WindowHost::new(OverlayHost::new(SheetOpen { long: true })).into(),
            })
        },
        size,
    )
    .settle(std::time::Duration::from_millis(600))
    .save(&path);
    println!("[snapshot] wrote {path}");

    // The tabbed sheet with form rows, opened on mount: details, then security.
    for (file, security) in [
        ("feedback-form-sheet", false),
        ("feedback-form-security", true),
    ] {
        let path = format!("{out}/{file}.png");
        Snapshot::new(
            move || {
                Element::from(Themed {
                    child: WindowHost::new(OverlayHost::new(FormSheetOpen { security })).into(),
                })
            },
            size,
        )
        .settle(std::time::Duration::from_millis(600))
        .save(&path);
        println!("[snapshot] wrote {path}");
    }

    // The menu opened from the Menus group at the top of the Buttons page.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Buttons,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/feedback-menu.png");
    shot.settle(std::time::Duration::from_millis(400))
        .click(890.0, 215.0)
        .settle(std::time::Duration::from_millis(500))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // Keyboard navigation: click Buttons in the sidebar, then ArrowDown
    // twice should land on Feedback.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Controls,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/navigation-keyboard.png");
    shot.settle(std::time::Duration::from_millis(300))
        .click(120.0, 118.0)
        .key(Key::Named(NamedKey::ArrowDown))
        .key(Key::Named(NamedKey::ArrowDown))
        .settle(std::time::Duration::from_millis(300))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // A narrow window: the page's Back button opens the list of pages.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Controls,
                    }),
            )
        },
        (600.0, 640.0),
    );
    let path = format!("{out}/navigation-overlay.png");
    shot.settle(std::time::Duration::from_millis(300))
        .click(24.0, 24.0)
        .settle(std::time::Duration::from_millis(400))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // A chart under the pointer: the crosshair and tooltip follow it.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .child(Gallery {
                        initial: Page::Charts,
                    }),
            )
        },
        (WINDOW_SIZE.0, 1100.0),
    );
    let path = format!("{out}/charts-hover.png");
    shot.settle(std::time::Duration::from_millis(400))
        .hover(560.0, 210.0)
        .settle(std::time::Duration::from_millis(200))
        .save(&path);
    println!("[snapshot] wrote {path}");
}

/// Provides the tokens, as an application root does, for a capture that
/// does not start from the gallery.
#[derive(PartialEq)]
struct Themed {
    child: Element,
}

impl Component for Themed {
    fn render(&self) -> impl IntoElement {
        let tokens = bingux_ui::theme::provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg)])
            .child(self.child.clone())
    }
}

/// Opens a sheet on mount over the Feedback page, so the headless runner
/// shows it without a click. `long` makes the body taller than the sheet's
/// height cap, to check that the body scrolls.
#[derive(PartialEq)]
struct SheetOpen {
    long: bool,
}

impl Component for SheetOpen {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let tokens = use_tokens();
        let long = self.long;
        let name = use_state(String::new);
        use_side_effect(move || {
            window.open(popup("Add a network", SheetOpenBody { name, long }));
        });
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg)])
            .child(Gallery {
                initial: Page::Feedback,
            })
    }
}

/// The body of the sheet opened by [`SheetOpen`].
#[derive(PartialEq)]
struct SheetOpenBody {
    name: State<String>,
    long: bool,
}

impl Component for SheetOpenBody {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let tokens = use_tokens();
        let name = self.name;
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Save".into(),
                enabled: !name.read().trim().is_empty(),
                on_press: EventHandler::new(move |_: ()| window.close()),
            }),
        });
        let mut body = rect()
            .tw(&tokens, &[flex_col(), gap(S3)])
            .child(text_field(name, |_| {}).placeholder("Network name"));
        if self.long {
            for index in 0..14 {
                body = body.child(label().text(format!("Row {index}")));
            }
        }
        body
    }
}

/// Opens the tabbed sheet with form rows on mount. `security` starts it on
/// the Security tab.
#[derive(PartialEq)]
struct FormSheetOpen {
    security: bool,
}

impl Component for FormSheetOpen {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let tokens = use_tokens();
        let security = self.security;
        let name = use_state(String::new);
        let tab = use_state(move || String::from(if security { "security" } else { "details" }));
        let security_key = use_state(|| "personal".to_string());
        let password = use_state(String::new);
        let auto_connect = use_state(|| true);
        use_side_effect(move || {
            window.open(
                popup(
                    "Add a network",
                    pages::FormDemoBody {
                        name,
                        tab,
                        security: security_key,
                        password,
                        auto_connect,
                    },
                )
                .tabs(pages::FormDemoTabs { tab }),
            );
        });
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg)])
            .child(Gallery {
                initial: Page::Feedback,
            })
    }
}
