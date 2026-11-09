//! The portal source: `org.freedesktop.portal.Settings` on the session bus.
//!
//! `ReadAll` for the two namespaces toolkits read answers in one call, so the
//! window waits for one round trip, and only when a portal is there. The
//! values follow the Settings portal specification:
//!
//! | Namespace and key                                | Value                                  |
//! | ------------------------------------------------ | -------------------------------------- |
//! | `org.freedesktop.appearance` `color-scheme`      | `u`: 0 no preference, 1 dark, 2 light  |
//! | `org.freedesktop.appearance` `accent-color`      | `(ddd)` sRGB, outside 0..1 means unset |
//! | `org.freedesktop.appearance` `contrast`          | `u`: 0 no preference, 1 more           |
//! | `org.freedesktop.appearance` `reduced-motion`    | `u`: 0 no preference, 1 reduced        |
//! | `org.gnome.desktop.interface` `icon-theme`       | `s`                                    |
//! | `org.gnome.desktop.interface` `enable-animations`| `b`                                    |
//! | `org.gnome.desktop.interface` `text-scaling-factor` | `d`                                 |
//!
//! "No preference" says nothing, so the next source is asked. A portal that
//! answers with nothing useful (an error, or no namespace) is no portal.

use std::collections::HashMap;
use std::time::Duration;

use zbus::zvariant::OwnedValue;

use super::Reading;
use crate::platform::bus;
use crate::theme::Scheme;

pub(crate) const APPEARANCE: &str = "org.freedesktop.appearance";
pub(crate) const INTERFACE: &str = "org.gnome.desktop.interface";
/// The Settings portal's interface.
pub(crate) const SETTINGS: bus::Target = bus::Target {
    destination: "org.freedesktop.portal.Desktop",
    path: "/org/freedesktop/portal/desktop",
    interface: "org.freedesktop.portal.Settings",
};

const WITHIN: Duration = Duration::from_millis(1200);

type Namespaces = HashMap<String, HashMap<String, OwnedValue>>;

/// Read the Settings portal. `None` when there is no bus, no portal or no
/// answer in time.
pub(super) fn read() -> Option<Reading> {
    let all: Namespaces = bus::call(
        WITHIN,
        SETTINGS,
        "ReadAll",
        (vec![APPEARANCE.to_owned(), INTERFACE.to_owned()],),
    )
    .ok()?;
    Some(parse(&all))
}

/// What a `ReadAll` answer says. Pure.
pub(crate) fn parse(all: &Namespaces) -> Reading {
    let appearance = all.get(APPEARANCE);
    let interface = all.get(INTERFACE);
    let number = |namespace: Option<&HashMap<String, OwnedValue>>, key: &str| -> Option<u32> {
        namespace?.get(key)?.downcast_ref::<u32>().ok()
    };
    // 0 is "no preference": not an opinion.
    let flag = |value: Option<u32>| (value? == 1).then_some(true);
    Reading {
        scheme: match number(appearance, "color-scheme") {
            Some(1) => Some(Scheme::PreferDark),
            Some(2) => Some(Scheme::PreferLight),
            _ => None,
        },
        accent: appearance
            .and_then(|namespace| namespace.get("accent-color"))
            .and_then(|value| value.downcast_ref::<(f64, f64, f64)>().ok())
            .and_then(|(red, green, blue)| accent_from_colour(red, green, blue)),
        high_contrast: flag(number(appearance, "contrast")),
        reduced_motion: flag(number(appearance, "reduced-motion")).or_else(|| {
            // `enable-animations` is on unless the person turned it off.
            let on = interface?
                .get("enable-animations")?
                .downcast_ref::<bool>()
                .ok()?;
            (!on).then_some(true)
        }),
        text_scale: interface
            .and_then(|namespace| namespace.get("text-scaling-factor"))
            .and_then(|value| value.downcast_ref::<f64>().ok())
            .map(|scale| scale as f32),
        icon_theme: interface
            .and_then(|namespace| namespace.get("icon-theme"))
            .and_then(|value| value.downcast_ref::<&str>().ok())
            .map(str::to_owned),
    }
}

/// The accent a portal's colour names. The portal sends sRGB in 0..1, and a
/// component outside that range means the desktop has no accent set. Any
/// colour is accepted: the nearest of the accents bingux-ui draws is used.
pub(crate) fn accent_from_colour(red: f64, green: f64, blue: f64) -> Option<&'static str> {
    let in_range = |component: f64| (0.0..=1.0).contains(&component);
    (in_range(red) && in_range(green) && in_range(blue))
        .then(|| crate::theme::nearest_accent(red as f32, green as f32, blue as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Structure, Value};

    fn colour(red: f64, green: f64, blue: f64) -> OwnedValue {
        owned(Value::Structure(Structure::from((red, green, blue))))
    }

    fn owned<'a>(value: impl Into<Value<'a>>) -> OwnedValue {
        value
            .into()
            .try_to_owned()
            .expect("a value without file descriptors")
    }

    fn answer(
        appearance: Vec<(&str, OwnedValue)>,
        interface: Vec<(&str, OwnedValue)>,
    ) -> Namespaces {
        let table = |entries: Vec<(&str, OwnedValue)>| -> HashMap<String, OwnedValue> {
            entries
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect()
        };
        HashMap::from([
            (APPEARANCE.to_owned(), table(appearance)),
            (INTERFACE.to_owned(), table(interface)),
        ])
    }

    #[test]
    fn the_appearance_namespace_is_read() {
        let found = parse(&answer(
            vec![
                ("color-scheme", owned(2_u32)),
                // GNOME's purple, in the portal's 0..1 sRGB.
                ("accent-color", colour(0.569, 0.255, 0.675)),
                ("contrast", owned(1_u32)),
                ("reduced-motion", owned(1_u32)),
            ],
            vec![],
        ));
        assert_eq!(found.scheme, Some(Scheme::PreferLight));
        assert_eq!(found.accent, Some("purple"));
        assert_eq!(found.high_contrast, Some(true));
        assert_eq!(found.reduced_motion, Some(true));
    }

    #[test]
    fn no_preference_is_not_an_opinion() {
        let found = parse(&answer(
            vec![
                ("color-scheme", owned(0_u32)),
                ("contrast", owned(0_u32)),
                ("reduced-motion", owned(0_u32)),
            ],
            vec![],
        ));
        assert_eq!(found, Reading::default());
    }

    #[test]
    fn an_unset_accent_is_not_an_opinion() {
        // The Settings specification's sentinel: a component outside 0..1.
        let found = parse(&answer(
            vec![("accent-color", colour(-1.0, -1.0, -1.0))],
            vec![],
        ));
        assert_eq!(found.accent, None);
    }

    #[test]
    fn the_interface_namespace_gives_icons_text_and_animations() {
        let found = parse(&answer(
            vec![],
            vec![
                ("icon-theme", owned("Papirus")),
                ("text-scaling-factor", owned(1.5_f64)),
                ("enable-animations", owned(false)),
            ],
        ));
        assert_eq!(found.icon_theme.as_deref(), Some("Papirus"));
        assert_eq!(found.text_scale, Some(1.5));
        assert_eq!(found.reduced_motion, Some(true));
    }

    #[test]
    fn the_portals_own_reduced_motion_comes_before_the_animations_switch() {
        let off = parse(&answer(
            vec![("reduced-motion", owned(0_u32))],
            vec![("enable-animations", owned(false))],
        ));
        // 0 is no preference, so the animations switch is still consulted.
        assert_eq!(off.reduced_motion, Some(true));
        let on = parse(&answer(vec![], vec![("enable-animations", owned(true))]));
        assert_eq!(on.reduced_motion, None);
    }

    #[test]
    fn an_empty_answer_says_nothing() {
        assert_eq!(parse(&HashMap::new()), Reading::default());
    }

    #[test]
    fn a_value_of_the_wrong_type_says_nothing() {
        let found = parse(&answer(vec![("color-scheme", owned("dark"))], vec![]));
        assert_eq!(found.scheme, None);
    }

    /// A Settings portal that publishes what a Gnoblin session does: a light
    /// scheme, no accent (the unset sentinel), reduced motion, and an icon theme.
    struct Portal;

    #[zbus::interface(name = "org.freedesktop.portal.Settings")]
    impl Portal {
        fn read_all(&self, _namespaces: Vec<String>) -> Namespaces {
            answer(
                vec![
                    ("color-scheme", owned(2_u32)),
                    ("accent-color", colour(-1.0, -1.0, -1.0)),
                    ("contrast", owned(0_u32)),
                    ("reduced-motion", owned(1_u32)),
                ],
                vec![
                    ("icon-theme", owned("Papirus")),
                    ("text-scaling-factor", owned(1.0_f64)),
                ],
            )
        }
    }

    #[test]
    fn a_real_portal_answer_is_read_over_the_wire() {
        let bus = crate::platform::testing::private_bus!();
        let _portal = bus.serve(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            Portal,
        );
        let all: Namespaces = bus::call_on(
            &bus.client(),
            SETTINGS,
            "ReadAll",
            &(vec![APPEARANCE.to_owned(), INTERFACE.to_owned()],),
        )
        .expect("the portal answers");
        let found = parse(&all);
        assert_eq!(found.scheme, Some(Scheme::PreferLight));
        assert_eq!(found.accent, None);
        assert_eq!(found.reduced_motion, Some(true));
        assert_eq!(found.icon_theme.as_deref(), Some("Papirus"));
        assert_eq!(found.text_scale, Some(1.0));
    }

    #[test]
    fn the_portal_decides_what_it_knows_and_the_config_the_rest() {
        use crate::platform::appearance::{Source, config, resolve};
        let bus = crate::platform::testing::private_bus!();
        let _portal = bus.serve(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            Portal,
        );
        let all: Namespaces = bus::call_on(
            &bus.client(),
            SETTINGS,
            "ReadAll",
            &(vec![APPEARANCE.to_owned()],),
        )
        .expect("the portal answers");
        let found = resolve(&[
            (Source::Portal, parse(&all)),
            (
                Source::Config,
                config::parse("[appearance]\ncolor_scheme = \"dark\"\naccent = \"purple\"\n"),
            ),
        ]);
        // The portal knows the scheme, so the config's is not used. It has no
        // accent, so the config's is.
        assert_eq!(
            (found.scheme, found.from.scheme),
            (Scheme::PreferLight, Source::Portal)
        );
        assert_eq!(
            (found.accent, found.from.accent),
            ("purple", Source::Config)
        );
    }
}
