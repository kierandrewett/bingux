//! Accounts and greeter appearance for the Bingux Session Manager (bsm).
//!
//! * [`accounts`]: which accounts the greeter offers (AccountsService, NSS fallback, policy).
//! * [`record`]: the strictly validated appearance record a user's shell publishes.
//! * [`publish`]: [`publish::Publisher`], the atomic, revisioned cache under
//!   `/var/lib/bingux/greeter/<uid>/`.
//! * [`convert`], [`sandbox`], [`imgconv`], [`image_ops`]: the sandboxed `bsm-imgconv`
//!   converter and its parent side.
//! * [`profile`]: building and validating the greeter-facing `UserProfile`.
//!
//! See `docs/bsm/appearance.md` for behaviour, the privacy trade-off and the opt-out.

pub mod accounts;
pub mod convert;
pub mod image_ops;
pub mod imgconv;
pub mod profile;
pub mod publish;
pub mod record;
pub mod sandbox;
mod sys;
