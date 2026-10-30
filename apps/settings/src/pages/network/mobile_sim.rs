//! The short popups that ask for a SIM's codes: unlock with the PIN or the
//! PUK, turn the PIN lock on or off, change the PIN. Each runs `mmcli -i`.

use freya::prelude::*;

use super::ModemState;
use super::modem::{self, pin_error, puk_error};
use crate::pages::network::model::NetCtx;
use crate::system::{CommandError, background_then};
use crate::ui::{
    Group, PopupAction, PopupActions, Status, caption, form_field, popup, popup_column, status,
    use_popup_actions, use_window,
};
use bingux_ui::style::S6;
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};

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
    fn title(self) -> &'static str {
        match self {
            Self::Unlock | Self::Puk => "Unlock SIM",
            Self::Enable => "Lock SIM with PIN",
            Self::Disable => "Turn off SIM lock",
            Self::Change => "Change PIN",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Unlock | Self::Puk => "Unlock",
            Self::Enable => "Turn on",
            Self::Disable => "Turn off",
            Self::Change => "Change",
        }
    }

    fn done(self) -> &'static str {
        match self {
            Self::Unlock | Self::Puk => "SIM unlocked.",
            Self::Enable => "SIM lock is on.",
            Self::Disable => "SIM lock is off.",
            Self::Change => "PIN changed.",
        }
    }

    /// The first field: the PIN, or the PUK for a blocked SIM.
    fn first_label(self) -> &'static str {
        match self {
            Self::Puk => "PUK",
            Self::Change => "Current PIN",
            _ => "PIN",
        }
    }

    /// Whether the popup also asks for a new PIN, twice.
    fn new_pin(self) -> bool {
        matches!(self, Self::Puk | Self::Change)
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Unlock => "Enter the PIN of your SIM.",
            Self::Puk => {
                "The SIM is blocked after too many wrong PINs. Enter the 8-digit PUK from your \
                 carrier, then choose a new PIN."
            }
            Self::Enable => {
                "With the SIM lock on, the PIN is asked for each time the phone starts."
            }
            Self::Disable => "Enter the PIN to stop asking for it when the phone starts.",
            Self::Change => "Choose a PIN of 4 to 8 digits.",
        }
    }
}

/// Open the popup for `task` on the SIM `sim`.
pub fn open(ctx: NetCtx, modem: ModemState, task: PinTask, sim: String) {
    ctx.window.open(
        popup(
            task.title(),
            PinBody {
                ctx,
                modem,
                task,
                sim,
            },
        )
        .flags(PopupFlags::FORM | PopupFlags::NO_DISMISS_OUTSIDE),
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
        let window = use_window();
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
            .then(|| "The two PINs are not the same.".to_string());
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
                            window.close();
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
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: task.action().into(),
                enabled: valid && !*busy.read(),
                on_press: EventHandler::new(move |_: ()| press()),
            }),
        });

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
                    form_field("New PIN", new_pin)
                        .password()
                        .validate(pin_error),
                )
                .row(
                    form_field("Repeat new PIN", repeat)
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
        popup_column()
            .child(fields.description(task.hint()))
            .child(bingux_ui::motion::reveal_block_if(
                S6,
                error
                    .read()
                    .clone()
                    .map(|text| Element::from(status(Status::Error(text)))),
            ))
    }
}

/// How many tries are left, in a sentence.
pub fn attempts_text(task: PinTask, left: u32) -> String {
    let what = if task == PinTask::Puk { "PUK" } else { "PIN" };
    match left {
        0 => format!("No {what} attempts are left."),
        1 => format!("1 {what} attempt left."),
        many => format!("{many} {what} attempts left."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_are_counted_in_words() {
        assert_eq!(attempts_text(PinTask::Unlock, 3), "3 PIN attempts left.");
        assert_eq!(attempts_text(PinTask::Unlock, 1), "1 PIN attempt left.");
        assert_eq!(attempts_text(PinTask::Puk, 10), "10 PUK attempts left.");
        assert_eq!(
            attempts_text(PinTask::Unlock, 0),
            "No PIN attempts are left."
        );
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
