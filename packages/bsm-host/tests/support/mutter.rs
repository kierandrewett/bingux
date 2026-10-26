//! A private, disposable hosted-session environment: its own runtime
//! directory, session bus and PipeWire, plus either the real hosted Gnoblin
//! (`BSM_TEST_GNOBLIN=/path/to/gnoblin`) or stock headless Mutter with a
//! stub for the `org.gnoblin.Hosted1` readiness interface. Nothing here
//! touches the host session.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct Hosted1Stub;

#[zbus::interface(name = "org.gnoblin.Hosted1")]
impl Hosted1Stub {
    #[zbus(property)]
    fn ready(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn wayland_display(&self) -> String {
        "wl-hosted".into()
    }
}

pub struct HostedEnv {
    pub dir: PathBuf,
    pub runtime: PathBuf,
    pub bus: String,
    pub display: String,
    children: Vec<Child>,
    _stub: Option<zbus::blocking::Connection>,
    pub real_gnoblin: bool,
}

fn have(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

pub fn available() -> Result<(), String> {
    for b in ["dbus-daemon", "pipewire", "wireplumber", "mutter", "weston-clickdot", "foot"] {
        if !have(b) {
            return Err(format!("{b} not installed"));
        }
    }
    Ok(())
}

impl HostedEnv {
    pub fn start(name: &str) -> HostedEnv {
        let dir = std::env::temp_dir().join(format!("bsm-host-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let runtime = dir.join("run");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        let bus_path = dir.join("bus");
        let bus = format!("unix:path={}", bus_path.display());
        let mut children = Vec::new();
        let log = |n: &str| std::fs::File::create(dir.join(format!("{n}.log"))).unwrap();

        children.push(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", &format!("--address={bus}")])
                .env("XDG_RUNTIME_DIR", &runtime)
                .stdout(log("dbus"))
                .stderr(log("dbus-err"))
                .spawn()
                .expect("dbus-daemon"),
        );
        wait_until(Duration::from_secs(5), || bus_path.exists(), "session bus");
        children.push(
            Command::new("pipewire")
                .env("XDG_RUNTIME_DIR", &runtime)
                .env("DBUS_SESSION_BUS_ADDRESS", &bus)
                .stdout(log("pipewire"))
                .stderr(log("pipewire-err"))
                .spawn()
                .expect("pipewire"),
        );
        wait_until(Duration::from_secs(5), || runtime.join("pipewire-0").exists(), "pipewire socket");

        // PipeWire links a capture stream to Mutter's node only with a session
        // manager, exactly as in a real user session.
        children.push(
            Command::new("wireplumber")
                .env("XDG_RUNTIME_DIR", &runtime)
                .env("DBUS_SESSION_BUS_ADDRESS", &bus)
                .stdout(log("wireplumber"))
                .stderr(log("wireplumber-err"))
                .spawn()
                .expect("wireplumber"),
        );
        std::thread::sleep(Duration::from_millis(800));

        let display = "wl-hosted".to_owned();
        let real = std::env::var_os("BSM_TEST_GNOBLIN").map(PathBuf::from);
        let mut stub = None;
        let mut cmd;
        if let Some(g) = &real {
            cmd = Command::new(g);
            cmd.args(["--hosted", "--wayland-display", &display]);
        } else {
            cmd = Command::new("mutter");
            cmd.args(["--headless", "--wayland", "--no-x11", &format!("--wayland-display={display}")]);
        }
        cmd.env("XDG_RUNTIME_DIR", &runtime)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus)
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .stdout(log("compositor"))
            .stderr(log("compositor-err"));
        children.push(cmd.spawn().expect("compositor"));

        // Wait for Mutter's services.
        let session = zbus::blocking::connection::Builder::address(bus.as_str())
            .unwrap()
            .build()
            .expect("connect to the private bus");
        let dbus = zbus::blocking::fdo::DBusProxy::new(&session).unwrap();
        wait_until(
            Duration::from_secs(30),
            || {
                ["org.gnome.Mutter.ScreenCast", "org.gnome.Mutter.RemoteDesktop", "org.gnome.Mutter.DisplayConfig"]
                    .iter()
                    .all(|n| dbus.name_has_owner(zbus::names::BusName::try_from(*n).unwrap()).unwrap_or(false))
            },
            "mutter bus names",
        );
        if real.is_none() {
            stub = Some(
                zbus::blocking::connection::Builder::address(bus.as_str())
                    .unwrap()
                    .name("org.gnoblin.Hosted1")
                    .unwrap()
                    .serve_at("/org/gnoblin/Hosted1", Hosted1Stub)
                    .unwrap()
                    .build()
                    .expect("serve Hosted1 stub"),
            );
        }
        wait_until(Duration::from_secs(10), || runtime.join(&display).exists(), "wayland socket");
        HostedEnv { dir, runtime, bus, display, children, _stub: stub, real_gnoblin: real.is_some() }
    }

    /// Run a Wayland client inside the hosted Gnoblin.
    pub fn client(&self, program: &str, args: &[&str]) -> Child {
        Command::new(program)
            .args(args)
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .env("WAYLAND_DISPLAY", &self.display)
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus)
            .env_remove("DISPLAY")
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(self.dir.join(format!("{}.err", program.rsplit('/').next().unwrap()))).unwrap())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {program}: {e}"))
    }

    pub fn log(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.join(name)).unwrap_or_default()
    }
}

impl Drop for HostedEnv {
    fn drop(&mut self) {
        for c in self.children.iter_mut().rev() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if std::env::var_os("BSM_HOST_KEEP_ENV").is_none() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

pub fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool, what: &str) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn _path(_: &Path) {}
