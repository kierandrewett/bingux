//! The body of the effects popup: a toolbar (the master switch, the Output and
//! Input switcher and the presets), and under it the stack beside the page of
//! the effect that is selected.

use bingux_audio_effects::Direction;
use bingux_ui::prelude::{
    HeaderBar, PopupAction, PopupActions, WindowLike, master_detail, use_popup_actions,
    view_switcher,
};
use bingux_ui::style::*;
use freya::prelude::*;

use super::{cached, page, presets, stack, use_fx};
use crate::ui::{caption, hstack, switch, use_tokens};
use bingux_ui::prelude::tr;

#[derive(PartialEq)]
pub(super) struct EffectsBody;

impl Component for EffectsBody {
    fn render(&self) -> impl IntoElement {
        let fx = use_fx(Direction::Output);
        let window = fx.window;
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: tr!("sound-effects-done"),
                enabled: true,
                on_press: EventHandler::new(move |()| window.close()),
            }),
        });
        let tokens = use_tokens();
        let data = fx.data.read().clone();
        let runtime = fx.runtime.read().clone();
        let direction = data.direction;
        let enabled = data.saved.of(direction).enabled;

        // Output and Input decide which chain the popup shows, so they are the
        // toolbar's switcher, above the hairline, not a control of the chain.
        let views = view_switcher(
            vec![
                ("output".to_string(), tr!("sound-effects-tab-output")),
                ("input".to_string(), tr!("sound-effects-tab-input")),
            ],
            direction.name(),
            move |name: String| {
                if let Some(next) = Direction::parse(&name) {
                    let (mut data, mut selected, mut open) = (fx.data, fx.selected, fx.page_open);
                    data.set(cached(next));
                    selected.set(None);
                    open.set(false);
                    fx.reload();
                    fx.refresh();
                }
            },
        );

        let bar = HeaderBar::new("")
            .draggable(false)
            .leading(
                hstack(S2)
                    .tw_layout(&[items(Align::Center)])
                    .child(switch(enabled, move |on: bool| fx.set_enabled(on)))
                    .maybe_child((!is_compact()).then(|| caption(&tr!("sound-effects")))),
            )
            .switcher(views)
            .trailing(presets::menu(fx, &data));

        let selected = fx.selected_id(&data);
        let master = ScrollView::new()
            .width(Size::fill())
            .height(Size::fill())
            .child(stack::StackView {
                fx,
                data: data.clone(),
                runtime: runtime.clone(),
                selected: selected.clone(),
            });
        let detail = ScrollView::new()
            .width(Size::fill())
            .height(Size::fill())
            .child(page::PageView {
                fx,
                data: data.clone(),
                runtime,
                selected,
            });
        let mut open = fx.page_open;
        let split = master_detail(master, detail, *fx.page_open.read(), move |()| {
            open.set(false)
        })
        .back_label(&tr!("sound-effects"));
        let content = rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col()])
            .child(bar)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(split),
            );
        content
    }
}
