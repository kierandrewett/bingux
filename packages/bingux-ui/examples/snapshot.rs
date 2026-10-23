//! Renders every gallery page headlessly to `/tmp/bingux-ui-gallery/`.
//!
//! Run with `cargo run --example snapshot --features testing`.

#[path = "gallery/pages.rs"]
mod pages;

use bingux_ui::controls::text_field::text_field;
use bingux_ui::prelude::{OverlayHost, sheet, use_overlays};
use bingux_ui::scale::S3;
use bingux_ui::snapshot::{Snapshot, snapshot};
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
    for page in Page::ALL {
        let name = format!("{page:?}").to_lowercase();
        let path = format!("{out}/{name}.png");
        snapshot(
            move || {
                Element::from(
                    rect()
                        .expanded()
                        .background(Color::from_rgb(28, 28, 32))
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
                    .background(Color::from_rgb(28, 28, 32))
                    .child(Gallery {
                        initial: Page::Controls,
                    }),
            )
        },
        (WINDOW_SIZE.0, 1100.0),
    );
    shot.settle(std::time::Duration::from_millis(400))
        .click(850.0, 606.0)
        .settle(std::time::Duration::from_millis(300))
        .hover(780.0, 730.0)
        .settle(std::time::Duration::from_millis(100))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // A control inside a card under the pointer.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .background(Color::from_rgb(28, 28, 32))
                    .child(Gallery {
                        initial: Page::Controls,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/controls-hover.png");
    shot.settle(std::time::Duration::from_millis(400))
        .hover(850.0, 606.0)
        .settle(std::time::Duration::from_millis(200))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // A subpage opened from a navigating row, with the header's Back button.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .background(Color::from_rgb(28, 28, 32))
                    .child(Gallery {
                        initial: Page::Rows,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/rows-subpage.png");
    shot.settle(std::time::Duration::from_millis(400))
        .click(630.0, 251.0)
        .settle(std::time::Duration::from_millis(400))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // The alert dialog open, then a toast after it is dismissed.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .background(Color::from_rgb(28, 28, 32))
                    .child(Gallery {
                        initial: Page::Feedback,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/feedback-dialog.png");
    shot.settle(std::time::Duration::from_millis(400))
        .click(919.0, 563.0)
        .settle(std::time::Duration::from_millis(400))
        .save(&path);
    println!("[snapshot] wrote {path}");
    let path = format!("{out}/feedback-toast.png");
    shot.click(60.0, 500.0)
        .settle(std::time::Duration::from_millis(300))
        .click(919.0, 604.0)
        .settle(std::time::Duration::from_millis(500))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // The sheet open on mount with one field: it should size to its content.
    let path = format!("{out}/feedback-sheet.png");
    Snapshot::new(
        move || Element::from(OverlayHost::new(SheetOpen { long: false })),
        size,
    )
    .settle(std::time::Duration::from_millis(600))
    .save(&path);
    println!("[snapshot] wrote {path}");

    // The same sheet with a body taller than the cap: the body should scroll.
    let path = format!("{out}/feedback-sheet-long.png");
    Snapshot::new(
        move || Element::from(OverlayHost::new(SheetOpen { long: true })),
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
            move || Element::from(OverlayHost::new(FormSheetOpen { security })),
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
                    .background(Color::from_rgb(28, 28, 32))
                    .child(Gallery {
                        initial: Page::Buttons,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/feedback-menu.png");
    shot.settle(std::time::Duration::from_millis(400))
        .click(900.0, 112.0)
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
                    .background(Color::from_rgb(28, 28, 32))
                    .child(Gallery {
                        initial: Page::Controls,
                    }),
            )
        },
        size,
    );
    let path = format!("{out}/navigation-keyboard.png");
    shot.settle(std::time::Duration::from_millis(300))
        .click(120.0, 150.0)
        .key(Key::Named(NamedKey::ArrowDown))
        .key(Key::Named(NamedKey::ArrowDown))
        .settle(std::time::Duration::from_millis(300))
        .save(&path);
    println!("[snapshot] wrote {path}");

    // The overlay sidebar in a narrow window, opened from the header.
    let mut shot = Snapshot::new(
        move || {
            Element::from(
                rect()
                    .expanded()
                    .background(Color::from_rgb(28, 28, 32))
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
                    .background(Color::from_rgb(28, 28, 32))
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

/// Opens a sheet on mount over the Feedback page, so the headless runner
/// shows it without a click. `long` makes the body taller than the sheet's
/// height cap, to check that the body scrolls.
#[derive(PartialEq)]
struct SheetOpen {
    long: bool,
}

impl Component for SheetOpen {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let long = self.long;
        let name = use_state(String::new);
        use_side_effect(move || {
            let mut body = rect()
                .spacing(S3)
                .child(text_field(name, |_| {}).placeholder("Network name"));
            if long {
                for index in 0..14 {
                    body = body.child(label().text(format!("Row {index}")));
                }
            }
            overlays.show_sheet(
                sheet("Add a network", body)
                    .primary("Save", move |_| overlays.close_sheet())
                    .primary_enabled(!name.read().trim().is_empty()),
            );
        });
        rect()
            .expanded()
            .background(Color::from_rgb(28, 28, 32))
            .child(Gallery {
                initial: Page::Feedback,
            })
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
        let overlays = use_overlays();
        let security = self.security;
        let name = use_state(String::new);
        let tab = use_state(move || String::from(if security { "security" } else { "details" }));
        let security_key = use_state(|| "personal".to_string());
        let password = use_state(String::new);
        let auto_connect = use_state(|| true);
        use_side_effect(move || {
            overlays.show_sheet(
                sheet(
                    "Add a network",
                    pages::FormDemoBody {
                        name,
                        tab,
                        security: security_key,
                        password,
                        auto_connect,
                    },
                )
                .tabs(pages::FormDemoTabs { tab })
                .primary("Save", move |_| overlays.close_sheet())
                .primary_enabled(!name.read().trim().is_empty()),
            );
        });
        rect()
            .expanded()
            .background(Color::from_rgb(28, 28, 32))
            .child(Gallery {
                initial: Page::Feedback,
            })
    }
}
