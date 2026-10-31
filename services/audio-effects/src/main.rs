//! `bingux-effects`: the command line of the sound effects engine.
//!
//! ```text
//! bingux-effects run <output|input>       run the engine (what the systemd unit does)
//! bingux-effects restore                  start or stop each direction as the settings say
//! bingux-effects reload <output|input>    read the preset again (what Settings does after an edit)
//! bingux-effects render <output|input>    print the configuration of every stage
//! bingux-effects scan                     list the plugins found, as JSON
//! bingux-effects status <output|input>    what the running stack reports, as JSON
//! bingux-effects presets <output|input>   list presets
//! bingux-effects import FILE <output|input> [NAME]
//!                                         read an EasyEffects, AutoEq or Bingux preset
//! ```

use std::process::ExitCode;

use bingux_audio_effects::graph::{self, Role, Target};
use bingux_audio_effects::preset::Direction;
use bingux_audio_effects::store::Store;
use bingux_audio_effects::{Catalog, effects, import, service};

const USAGE: &str = "usage: bingux-effects run|reload|render|status|presets <output|input> | restore | scan | import FILE <output|input> [NAME]";

fn direction(argument: Option<&String>) -> Result<Direction, String> {
    argument
        .and_then(|text| Direction::parse(text))
        .ok_or_else(|| USAGE.to_string())
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let store = Store::default();
    let result = match arguments.first().map(String::as_str) {
        Some("run") => {
            direction(arguments.get(1)).and_then(|direction| service::run(&store, direction))
        }
        Some("reload") => direction(arguments.get(1)).and_then(service::reload),
        Some("restore") => store.ensure_factory().map_err(|e| e.0).and_then(|()| {
            for direction in Direction::ALL {
                // Only the directions that are on need starting: the
                // systemd units that are off were disabled when they were turned off.
                if store.state().of(direction).enabled {
                    service::start(direction)?;
                }
            }
            Ok(())
        }),
        Some("render") => direction(arguments.get(1)).map(|direction| {
            let state = store.state();
            let device = state.of(direction).device.clone();
            let preset = service::active_preset(&store, direction, device.as_deref());
            let catalog = Catalog::load();
            let target = Target { device };
            println!(
                "### head\n{}",
                graph::render_end(direction, &Role::Head, &target).config
            );
            for slot in &preset.plugins {
                let Some(effect) = effects::find(&slot.kind) else {
                    eprintln!("skipped {}: not an effect Bingux offers", slot.id);
                    continue;
                };
                match graph::render_effect(direction, slot, effect, catalog.plugin_of(effect)) {
                    Ok(rendered) => println!("### {}\n{}", slot.id, rendered.config),
                    Err(skipped) => eprintln!("skipped {}: {}", skipped.id, skipped.reason),
                }
            }
            println!(
                "### tail\n{}",
                graph::render_end(direction, &Role::Tail, &target).config
            );
        }),
        Some("status") => direction(arguments.get(1)).and_then(|direction| {
            service::read_status(direction)
                .map(|status| {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&status).unwrap_or_default()
                    )
                })
                .ok_or_else(|| "The effects are not running.".to_string())
        }),
        Some("presets") => direction(arguments.get(1)).map(|direction| {
            let _ = store.ensure_factory();
            for name in store.list(direction) {
                println!("{name}");
            }
        }),
        Some("scan") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&Catalog::load()).unwrap_or_default()
            );
            Ok(())
        }
        Some("import") => match (arguments.get(1), direction(arguments.get(2))) {
            (Some(file), Ok(direction)) => {
                let name = arguments.get(3).cloned().unwrap_or_else(|| {
                    std::path::Path::new(file).file_stem().map_or_else(
                        || "Imported".to_string(),
                        |stem| stem.to_string_lossy().into_owned(),
                    )
                });
                std::fs::read_to_string(file)
                    .map_err(|e| e.to_string())
                    .and_then(|text| {
                        import::read(&text, &name, direction, &Catalog::load()).map_err(|e| e.0)
                    })
                    .and_then(|imported| {
                        for item in imported {
                            let mut preset = item.preset;
                            preset.name = store.free_name(preset.direction, &preset.name);
                            store.save(&preset).map_err(|e| e.0)?;
                            println!("imported \"{}\" ({})", preset.name, item.mapped.join(", "));
                            for line in item.unmapped {
                                println!("  not mapped: {line}");
                            }
                        }
                        Ok(())
                    })
            }
            _ => Err(USAGE.to_string()),
        },
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("bingux-effects: {message}");
            ExitCode::FAILURE
        }
    }
}
