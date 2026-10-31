//! The body of the effects popup: a top bar (the master switch, the Output and
//! Input tabs and the presets), and under it the stack beside the page of the
//! effect that is selected.

use bingux_audio_effects::Direction;
use bingux_ui::prelude::{
    PopupAction, PopupActions, WindowLike, master_detail, segmented, use_popup_actions,
};
use bingux_ui::style::*;
use freya::prelude::*;

use super::{cached, page, presets, stack, use_fx};
use crate::ui::{caption, hstack, switch, use_tokens};

#[derive(PartialEq)]
pub(super) struct EffectsBody;

impl Component for EffectsBody {
    fn render(&self) -> impl IntoElement {
        let fx = use_fx(Direction::Output);
        let window = fx.window;
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Done".to_string(),
                enabled: true,
                on_press: EventHandler::new(move |()| window.close()),
            }),
        });
        let tokens = use_tokens();
        let data = fx.data.read().clone();
        let runtime = fx.runtime.read().clone();
        let direction = data.direction;
        let enabled = data.saved.of(direction).enabled;

        let tabs = segmented(
            vec![
                ("output".to_string(), "Output".to_string()),
                ("input".to_string(), "Input".to_string()),
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
        )
        .tabs();

        let bar = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    items(Align::Center),
                    justify(Align::Between),
                    gap(S3),
                    px(S4),
                    py(S3),
                ],
            )
            .child(
                hstack(S2)
                    .tw_layout(&[items(Align::Center)])
                    .child(switch(enabled, move |on: bool| fx.set_enabled(on)))
                    .child(caption("Effects")),
            )
            .child(tabs)
            .child(presets::menu(fx, &data));

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
        .back_label("Effects");
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
