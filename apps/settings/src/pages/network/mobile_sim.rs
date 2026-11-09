//! The form dialogs that ask for a SIM's codes: unlock with the PIN or the
//! PUK, turn the PIN lock on or off, change the PIN. Each runs `mmcli -i`.

use freya::prelude::*;

use super::ModemState;
use super::modem::{self, pin_error, puk_error};
use crate::pages::network::model::NetCtx;
use crate::system::{CommandError, background_then};
use crate::ui::{Group, Status, form_field, status};
use bingux_ui::prelude::tr;
use bingux_ui::shell::{form_dialog, open_form};
use bingux_ui::style::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PinTask {
    /// Send the PIN of a locked SIM.
    Unlock,
    /// Send the PUK and a new PIN to a blocked SIM.
    Puk,
    /// Ask for the PIN when the phone starts.
    Enable,
    Disable,
    Change,
}

impl PinTask {
    fn title(self) -> String {
        match self {
            Self::Unlock | Self::Puk => tr!("network-mobile-sim-unlock-title"),
            Self::Enable => tr!("network-mobile-sim-enable-title"),
            Self::Disable => tr!("network-mobile-sim-disable-title"),
            Self::Change => tr!("network-mobile-sim-change-title"),
        }
    }

    fn action(self) -> String {
        match self {
            Self::Unlock | Self::Puk => tr!("network-mobile-sim-unlock-action"),
            Self::Enable => tr!("network-mobile-sim-enable-action"),
            Self::Disable => tr!("network-mobile-sim-disable-action"),
            Self::Change => tr!("network-mobile-sim-change-action"),
        }
    }

    fn done(self) -> String {
        match self {
            Self::Unlock | Self::Puk => tr!("network-mobile-sim-unlocked-toast"),
            Self::Enable => tr!("network-mobile-sim-enabled-toast"),
            Self::Disable => tr!("network-mobile-sim-disabled-toast"),
            Self::Change => tr!("network-mobile-sim-changed-toast"),
        }
    }

    /// The first field: the PIN, or the PUK for a blocked SIM.
    fn first_label(self) -> String {
        match self {
            Self::Puk => tr!("network-mobile-sim-puk-label"),
            Self::Change => tr!("network-mobile-sim-current-pin-label"),
            _ => tr!("network-mobile-sim-pin-label"),
        }
    }

    /// Whether the dialog also asks for a new PIN, twice.
    fn new_pin(self) -> bool {
        matches!(self, Self::Puk | Self::Change)
    }

    fn hint(self) -> String {
        match self {
            Self::Unlock => tr!("network-mobile-sim-unlock-hint"),
            Self::Puk => tr!("network-mobile-sim-puk-hint"),
            Self::Enable => tr!("network-mobile-sim-enable-hint"),
            Self::Disable => tr!("network-mobile-sim-disable-hint"),
            Self::Change => tr!("network-mobile-sim-change-hint"),
        }
    }
}

/// Open the dialog for `task` on the SIM `sim`.
pub fn open(ctx: NetCtx, modem: ModemState, task: PinTask, sim: String) {
    open_form(
        ctx.overlays,
        task.title(),
        PinBody {
            ctx,
            modem,
            task,
            sim,
        },
        None,
    );
}

/// Run the command of `task` with what was typed.
fn send(task: PinTask, sim: &str, first: &str, new_pin: &str) -> Result<(), CommandError> {
    match task {
        PinTask::Unlock => modem::unlock_pin(sim, first),
        PinTask::Puk => modem::unlock_puk(sim, first, new_pin),
        PinTask::Enable => modem::set_pin_lock(sim, first, true),
        PinTask::Disable => modem::set_pin_lock(sim, first, false),
        PinTask::Change => modem::change_pin(sim, first, new_pin),
    }
}

#[derive(PartialEq)]
struct PinBody {
    ctx: NetCtx,
    modem: ModemState,
    task: PinTask,
    sim: String,
}

impl Component for PinBody {
    fn render(&self) -> impl IntoElement {
        let (ctx, task, modem_state) = (self.ctx, self.task, self.modem);
        let first = use_state(String::new);
        let new_pin = use_state(String::new);
        let repeat = use_state(String::new);
        let error = use_state(|| None::<String>);
        let busy = use_state(|| false);

        let first_check = if task == PinTask::Puk {
            puk_error
        } else {
            pin_error
        };
        let mismatch = (!repeat.read().is_empty() && *repeat.read() != *new_pin.read())
            .then(|| tr!("network-mobile-sim-pin-mismatch-error"));
        let valid = first_check(&first.read()).is_ok()
            && (!task.new_pin()
                || (pin_error(&new_pin.read()).is_ok() && *repeat.read() == *new_pin.read()));

        let sim = self.sim.clone();
        let submit = move || {
            if !valid || *busy.read() {
                return;
            }
            let mut busy = busy;
            busy.set(true);
            let mut error = error;
            error.set(None);
            let (code, replacement, sim) =
                (first.read().clone(), new_pin.read().clone(), sim.clone());
            background_then(
                move || send(task, &sim, &code, &replacement),
                move |result| {
                    let mut busy = busy;
                    busy.set(false);
                    super::reload_modem(modem_state);
                    match result {
                        Ok(()) => {
                            ctx.overlays.dismiss_dialog();
                            ctx.overlays.toast(task.done());
                        }
                        Err(failure) => {
                            let mut error = error;
                            error.set(Some(failure.0));
                        }
                    }
                },
            );
        };

        let press = submit.clone();
        let blocked = if *busy.read() {
            None
        } else {
            first_check(&first.read()).err().or_else(|| {
                if task.new_pin() {
                    pin_error(&new_pin.read())
                        .err()
                        .map(|text| tr!("network-mobile-sim-new-pin-error", error = text))
                        .or_else(|| {
                            (*repeat.read() != *new_pin.read())
                                .then(|| tr!("network-mobile-sim-pin-mismatch-error"))
                        })
                } else {
                    None
                }
            })
        };

        let attempts = modem_state
            .read()
            .clone()
            .and_then(Result::ok)
            .flatten()
            .and_then(|modem| modem.attempts_left());
        let key = submit.clone();
        let mut fields = Group::new(None::<String>).row(
            form_field(task.first_label(), first)
                .password()
                .autofocus()
                .validate(first_check)
                .on_submit(move |_| key()),
        );
        if task.new_pin() {
            let key = submit.clone();
            fields = fields
                .row(
                    form_field(tr!("network-mobile-sim-new-pin-label"), new_pin)
                        .password()
                        .validate(pin_error),
                )
                .row(
                    form_field(tr!("network-mobile-sim-repeat-pin-label"), repeat)
                        .password()
                        .error(mismatch)
                        .on_submit(move |_| key()),
                );
        }
        let note = matches!(task, PinTask::Unlock | PinTask::Puk)
            .then_some(attempts)
            .flatten()
            .map(|left| attempts_text(task, left));
        if let Some(note) = note {
            fields = fields.footer(note);
        }
        let body = rect()
            .tw(
                &bingux_ui::surface::tokens_now(),
                &[flex_col(), w_full(), gap(S4)],
            )
            .child(fields.description(task.hint()))
            .child(bingux_ui::motion::reveal_block_if(
                S6,
                error
                    .read()
                    .clone()
                    .map(|text| Element::from(status(Status::Error(text)))),
            ));
        form_dialog(task.title(), body, task.action(), move |_: ()| press())
            .blocked(blocked)
            .stays_open()
    }
}

/// How many tries are left, in a sentence.
pub fn attempts_text(task: PinTask, left: u32) -> String {
    let what = if task == PinTask::Puk {
        tr!("network-mobile-sim-puk-word")
    } else {
        tr!("network-mobile-sim-pin-word")
    };
    tr!(
        "network-mobile-sim-attempts-left",
        count = left,
        what = what
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_are_counted_in_words() {
        crate::register_catalogue_for_tests();
        assert_eq!(attempts_text(PinTask::Unlock, 3), "3 PIN attempts left");
        assert_eq!(attempts_text(PinTask::Unlock, 1), "1 PIN attempt left");
        assert_eq!(attempts_text(PinTask::Puk, 10), "10 PUK attempts left");
        assert_eq!(attempts_text(PinTask::Unlock, 0), "No PIN attempts left");
    }

    #[test]
    fn only_two_tasks_ask_for_a_new_pin() {
        let asking: Vec<PinTask> = [
            PinTask::Unlock,
            PinTask::Puk,
            PinTask::Enable,
            PinTask::Disable,
            PinTask::Change,
        ]
        .into_iter()
        .filter(|task| task.new_pin())
        .collect();
        assert_eq!(asking, vec![PinTask::Puk, PinTask::Change]);
    }
}
