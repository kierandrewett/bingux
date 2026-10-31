//! The effects Bingux offers, and the public plugin that implements each.
//!
//! The set is Easy Effects' own: where Easy Effects hosts a plugin from a
//! published project (Linux Studio Plugins, Calf Studio Gear, ZamAudio, MDA,
//! x42), Bingux hosts the same plugin; where it uses another public library
//! with no plugin of its own to host, the closest packaged plugin does the job
//! and the page says so. Nothing here is written by Bingux. Effects that
//! Easy Effects implements itself, or that need a library nobody packages as a
//! plugin, are listed in [`MISSING`] with the reason.
//!
//! `design/effects-stack.md` has the mapping with the packages and controls.

/// Where a plugin comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// An LV2 plugin, by URI.
    Lv2(&'static str),
    /// A LADSPA plugin: the library file name without `.so`, and the label.
    Ladspa {
        library: &'static str,
        label: &'static str,
    },
    /// PipeWire's own convolver.
    Convolver,
}

impl Source {
    /// The id the catalog knows the plugin by.
    pub fn plugin_id(&self) -> String {
        match self {
            Source::Lv2(uri) => (*uri).to_string(),
            Source::Ladspa { library, label } => format!("ladspa:{library}:{label}"),
            Source::Convolver => "builtin:convolver".to_string(),
        }
    }
}

/// The project that provides a plugin, and what to install to get it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Package {
    LinuxStudio,
    Calf,
    Zam,
    Mda,
    X42,
    Rubberband,
    PipeWire,
}

impl Package {
    /// The project's name, as the effect's page credits it.
    pub fn project(self) -> &'static str {
        match self {
            Package::LinuxStudio => "Linux Studio Plugins",
            Package::Calf => "Calf Studio Gear",
            Package::Zam => "ZamAudio",
            Package::Mda => "MDA",
            Package::X42 => "x42",
            Package::Rubberband => "Rubber Band",
            Package::PipeWire => "PipeWire",
        }
    }

    /// The Fedora package that ships it.
    pub fn fedora(self) -> &'static str {
        match self {
            Package::LinuxStudio => "lsp-plugins-lv2",
            Package::Calf => "lv2-calf-plugins",
            Package::Zam => "lv2-zam-plugins",
            Package::Mda => "lv2-mdala-plugins",
            Package::X42 => "lv2-x42-plugins",
            Package::Rubberband => "ladspa-rubberband-plugins",
            Package::PipeWire => "pipewire",
        }
    }
}

/// How many audio channels the plugin takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    /// One instance with a left and a right input and output.
    Stereo,
    /// One input and one output: the engine runs one instance for each side.
    Mono,
}

/// Where an effect is listed when adding one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Equalizer,
    Dynamics,
    Space,
    Enhance,
    Stereo,
    Pitch,
    Utility,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Equalizer,
        Category::Dynamics,
        Category::Space,
        Category::Enhance,
        Category::Stereo,
        Category::Pitch,
        Category::Utility,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::Equalizer => "Equalizers and filters",
            Category::Dynamics => "Dynamics",
            Category::Space => "Space and delay",
            Category::Enhance => "Enhancement",
            Category::Stereo => "Stereo",
            Category::Pitch => "Pitch",
            Category::Utility => "Level",
        }
    }
}

/// One effect a person can put in a stack.
#[derive(Clone, Copy, Debug)]
pub struct Effect {
    /// What a preset's `type` says.
    pub key: &'static str,
    pub name: &'static str,
    /// One plain sentence for the list of effects to add.
    pub summary: &'static str,
    pub category: Category,
    pub source: Source,
    pub package: Package,
    pub channels: Channels,
    /// The control that is the effect's input gain and output gain, for the two
    /// fields every page leads with.
    pub input_gain: Option<&'static str>,
    pub output_gain: Option<&'static str>,
    /// Which curated page lays the controls out; `generic` draws every control.
    pub layout: &'static str,
}

const fn lsp(uri: &'static str) -> Source {
    Source::Lv2(uri)
}

/// Every effect, in the order the list shows within a category.
pub const ALL: &[Effect] = &[
    Effect {
        key: "equalizer",
        name: "Equalizer",
        summary: "Raise or lower the bass, mids and treble with up to 32 filters.",
        category: Category::Equalizer,
        source: lsp("http://lsp-plug.in/plugins/lv2/para_equalizer_x32_lr"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "equalizer",
    },
    Effect {
        key: "midside_equalizer",
        name: "Mid/side equalizer",
        summary: "An equalizer for the centre of the sound and its sides separately.",
        category: Category::Equalizer,
        source: lsp("http://lsp-plug.in/plugins/lv2/para_equalizer_x32_ms"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "equalizer",
    },
    Effect {
        key: "filter",
        name: "Filter",
        summary: "Cut the low or high end, or a band, with a steep filter.",
        category: Category::Equalizer,
        source: lsp("http://lsp-plug.in/plugins/lv2/filter_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "generic",
    },
    Effect {
        key: "compressor",
        name: "Compressor",
        summary: "Make loud parts quieter so the volume stays even.",
        category: Category::Dynamics,
        source: lsp("http://lsp-plug.in/plugins/lv2/compressor_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "compressor",
    },
    Effect {
        key: "multiband_compressor",
        name: "Multiband compressor",
        summary: "Compress the bass, mids and treble each on their own.",
        category: Category::Dynamics,
        source: lsp("http://lsp-plug.in/plugins/lv2/mb_compressor_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "generic",
    },
    Effect {
        key: "limiter",
        name: "Limiter",
        summary: "Stop the sound going over a level, to protect speakers and ears.",
        category: Category::Dynamics,
        source: lsp("http://lsp-plug.in/plugins/lv2/limiter_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "limiter",
    },
    Effect {
        key: "gate",
        name: "Gate",
        summary: "Silence the sound when it falls below a level, such as room noise.",
        category: Category::Dynamics,
        source: lsp("http://lsp-plug.in/plugins/lv2/gate_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "gate",
    },
    Effect {
        key: "multiband_gate",
        name: "Multiband gate",
        summary: "Gate the bass, mids and treble each on their own.",
        category: Category::Dynamics,
        source: lsp("http://lsp-plug.in/plugins/lv2/mb_gate_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "generic",
    },
    Effect {
        key: "expander",
        name: "Expander",
        summary: "Make quiet parts quieter, a softer gate.",
        category: Category::Dynamics,
        source: lsp("http://lsp-plug.in/plugins/lv2/expander_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("g_in"),
        output_gain: Some("g_out"),
        layout: "generic",
    },
    Effect {
        key: "deesser",
        name: "De-esser",
        summary: "Soften harsh \"s\" sounds in a voice.",
        category: Category::Dynamics,
        source: Source::Lv2("http://calf.sourceforge.net/plugins/Deesser"),
        package: Package::Calf,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "maximizer",
        name: "Maximizer",
        summary: "Make everything louder without letting it clip.",
        category: Category::Dynamics,
        source: Source::Lv2("urn:zamaudio:ZaMaximX2"),
        package: Package::Zam,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "autogain",
        name: "Auto gain",
        summary: "Keep the loudness steady by raising or lowering the volume for you.",
        category: Category::Utility,
        source: lsp("http://lsp-plug.in/plugins/lv2/autogain_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "loudness",
        name: "Loudness",
        summary: "Keep bass and treble full at low volume.",
        category: Category::Utility,
        source: lsp("http://lsp-plug.in/plugins/lv2/loud_comp_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: Some("in_gain"),
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "bass_loudness",
        name: "Bass loudness",
        summary: "Add bass the ear loses when the volume is turned down.",
        category: Category::Utility,
        source: Source::Lv2("http://drobilla.net/plugins/mda/Loudness"),
        package: Package::Mda,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "delay",
        name: "Delay",
        summary: "Delay one side or both, to line up speakers or add an echo.",
        category: Category::Space,
        source: lsp("http://lsp-plug.in/plugins/lv2/comp_delay_x2_stereo"),
        package: Package::LinuxStudio,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "reverb",
        name: "Reverb",
        summary: "Add the sound of a room.",
        category: Category::Space,
        source: Source::Lv2("http://calf.sourceforge.net/plugins/Reverb"),
        package: Package::Calf,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "convolver",
        name: "Convolver",
        summary: "Shape the sound with an impulse response file, such as a speaker correction.",
        category: Category::Space,
        source: Source::Convolver,
        package: Package::PipeWire,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "convolver",
    },
    Effect {
        key: "bass_enhancer",
        name: "Bass enhancer",
        summary: "Add harmonics so the bass sounds fuller on small speakers.",
        category: Category::Enhance,
        source: Source::Lv2("http://calf.sourceforge.net/plugins/BassEnhancer"),
        package: Package::Calf,
        channels: Channels::Stereo,
        input_gain: Some("level_in"),
        output_gain: Some("level_out"),
        layout: "generic",
    },
    Effect {
        key: "exciter",
        name: "Exciter",
        summary: "Add sparkle to the high end.",
        category: Category::Enhance,
        source: Source::Lv2("http://calf.sourceforge.net/plugins/Exciter"),
        package: Package::Calf,
        channels: Channels::Stereo,
        input_gain: Some("level_in"),
        output_gain: Some("level_out"),
        layout: "generic",
    },
    Effect {
        key: "crusher",
        name: "Crusher",
        summary: "Make the sound crunchy by lowering its resolution.",
        category: Category::Enhance,
        source: Source::Lv2("http://calf.sourceforge.net/plugins/Crusher"),
        package: Package::Calf,
        channels: Channels::Stereo,
        input_gain: Some("level_in"),
        output_gain: Some("level_out"),
        layout: "generic",
    },
    Effect {
        key: "stereo_tools",
        name: "Stereo tools",
        summary: "Widen or narrow the stereo image, balance, mono and swap channels.",
        category: Category::Stereo,
        source: Source::Lv2("http://calf.sourceforge.net/plugins/StereoTools"),
        package: Package::Calf,
        channels: Channels::Stereo,
        input_gain: Some("level_in"),
        output_gain: Some("level_out"),
        layout: "generic",
    },
    Effect {
        key: "pitch",
        name: "Pitch",
        summary: "Raise or lower the pitch without changing the speed.",
        category: Category::Pitch,
        source: Source::Ladspa {
            library: "ladspa-rubberband",
            label: "rubberband-r3-pitchshifter-stereo",
        },
        package: Package::Rubberband,
        channels: Channels::Stereo,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
    Effect {
        key: "autotune",
        name: "Autotune",
        summary: "Pull a singing voice onto the nearest note.",
        category: Category::Pitch,
        source: Source::Lv2("http://gareus.org/oss/lv2/fat1"),
        package: Package::X42,
        channels: Channels::Mono,
        input_gain: None,
        output_gain: None,
        layout: "generic",
    },
];

impl Effect {
    /// The controls Calf and ZamAudio store as a linear gain but that people
    /// read in decibels: the plugin declares no unit for them, so the effect
    /// says which they are.
    pub fn linear_gains(&self) -> &'static [&'static str] {
        match self.key {
            "deesser" => &["threshold", "makeup", "f1_level", "f2_level"],
            "reverb" => &["level_in", "level_out", "amount", "dry"],
            "bass_enhancer" | "exciter" => &["level_in", "level_out", "amount"],
            "crusher" => &["level_in", "level_out"],
            "stereo_tools" => &["level_in", "level_out", "slev", "mlev"],
            _ => &[],
        }
    }

    /// The settings a new instance of the effect starts with, over the
    /// plugin's own defaults: Easy Effects' ten equalizer bands.
    pub fn initial_controls(&self) -> Vec<(String, f64)> {
        match self.layout {
            "equalizer" if self.key == "equalizer" => {
                crate::eq::default_controls().into_iter().collect()
            }
            "equalizer" => {
                let mut controls = Vec::new();
                for (index, &frequency) in crate::eq::DEFAULT_FREQUENCIES.iter().enumerate() {
                    for side in [crate::eq::Side::First, crate::eq::Side::Second] {
                        let symbols = crate::eq::Symbols::of(index, side, true);
                        controls.push((symbols.kind, crate::eq::BandKind::Bell.code()));
                        controls.push((symbols.frequency, frequency));
                        controls.push((symbols.quality, crate::eq::DEFAULT_Q));
                    }
                }
                controls
            }
            _ => Vec::new(),
        }
    }
}

/// An Easy Effects effect Bingux cannot offer, and why.
#[derive(Clone, Copy, Debug)]
pub struct Missing {
    pub name: &'static str,
    pub reason: &'static str,
}

pub const MISSING: &[Missing] = &[
    Missing {
        name: "Crossfeed",
        reason: "Easy Effects uses libbs2b in its own code. Fedora ships the library only, with no plugin to host (Debian has a LADSPA one).",
    },
    Missing {
        name: "Crystalizer",
        reason: "Easy Effects' own algorithm; no published plugin does the same.",
    },
    Missing {
        name: "Noise reduction (RNNoise)",
        reason: "No distribution packages a plugin for RNNoise; the library alone cannot be hosted.",
    },
    Missing {
        name: "Noise reduction (DeepFilterNet)",
        reason: "No distribution packages its LADSPA plugin.",
    },
    Missing {
        name: "Noise reduction (Speex)",
        reason: "SpeexDSP's preprocessor has no plugin wrapper to host.",
    },
    Missing {
        name: "Echo canceller",
        reason: "WebRTC's canceller needs the sound being played as a reference, which a chain does not have.",
    },
    Missing {
        name: "Voice suppressor",
        reason: "Easy Effects' own algorithm; no published plugin does the same.",
    },
    Missing {
        name: "Crosstalk canceller",
        reason: "Easy Effects' own algorithm; no published plugin does the same.",
    },
    Missing {
        name: "Harmonizer",
        reason: "Easy Effects uses Rakarrack Plus, which no distribution packages as a plugin.",
    },
];

/// The effect with preset type `key`.
pub fn find(key: &str) -> Option<&'static Effect> {
    ALL.iter().find(|effect| effect.key == key)
}

/// The effects of `category`, in list order.
pub fn in_category(category: Category) -> impl Iterator<Item = &'static Effect> {
    ALL.iter().filter(move |effect| effect.category == category)
}
