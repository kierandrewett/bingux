//! Renders every gallery page headlessly to `/tmp/bingux-ui-gallery/`.
//!
//! Run with `cargo run --example snapshot --features testing`.

#[path = "gallery/pages.rs"]
mod pages;

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
}
