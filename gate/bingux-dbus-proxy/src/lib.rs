//! D-Bus proxy for Bingux sandbox.
//!
//! Sits between sandboxed applications and the real D-Bus bus, filtering
//! messages based on per-package permission grants. Each sandboxed app
//! connects to its own proxy socket instead of the real bus.
//!
//! Architecture:
//! ```text
//! Sandboxed app → /run/bingux/dbus/<pkg>.sock (proxy) → real D-Bus bus
//! ```
//!
//! The proxy enforces:
//! - Which D-Bus interfaces the app can call
//! - Which object paths the app can access
//! - Whether the app can own bus names
//! - Rate limiting on sensitive operations

pub mod filter;
pub mod policy;
pub mod proxy;

pub use filter::{DbusFilter, FilterAction};
pub use policy::{DbusPolicy, PolicyRule};
pub use proxy::{ProxyConfig, ProxyInstance};
