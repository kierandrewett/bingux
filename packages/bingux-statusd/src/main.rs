mod gnoblin;

use bingux_statusd::{
    DesktopState, Metrics, OsdRequest, byte_rate, cpu_percent, metrics_with_desktop_state_json,
    osd_json, parse_cpu_stat, parse_meminfo, parse_network_totals,
};
use std::{
    env, fs,
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const METRICS_SOCKET_NAME: &str = "metrics-v1.sock";
const OSD_SOCKET_NAME: &str = "osd-v2.sock";
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const MAX_CLIENTS: usize = 16;
const EVENT_QUEUE_CAPACITY: usize = MAX_CLIENTS * 2 + 8;
const CLIENT_LISTENER_INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const CLIENT_LISTENER_MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
const ACTIVE_SOCKET_PROBE_TIMEOUT: Duration = Duration::from_millis(250);

pub(crate) enum Event {
    MetricsClient(UnixStream),
    OsdClient(UnixStream),
    DesktopState(DesktopState),
    OsdRequest(OsdRequest),
}

struct RawSample {
    cpu: bingux_statusd::CpuSample,
    memory: bingux_statusd::MemorySample,
    network: bingux_statusd::NetworkTotals,
    captured_at: Instant,
}

struct Sampler {
    previous: Option<RawSample>,
}

impl Sampler {
    fn new() -> Self {
        Self { previous: None }
    }

    fn sample(&mut self) -> io::Result<Metrics> {
        let current = RawSample {
            cpu: read_cpu_sample()?,
            memory: read_memory_sample()?,
            network: read_network_totals()?,
            captured_at: Instant::now(),
        };
        let metrics = if let Some(previous) = &self.previous {
            let elapsed = current.captured_at.duration_since(previous.captured_at);

            Metrics {
                cpu_percent: cpu_percent(previous.cpu, current.cpu),
                memory_total_bytes: current.memory.total_bytes,
                memory_used_bytes: current.memory.used_bytes,
                receive_bytes_per_second: byte_rate(
                    previous.network.receive_bytes,
                    current.network.receive_bytes,
                    elapsed,
                ),
                transmit_bytes_per_second: byte_rate(
                    previous.network.transmit_bytes,
                    current.network.transmit_bytes,
                    elapsed,
                ),
            }
        } else {
            Metrics {
                cpu_percent: None,
                memory_total_bytes: current.memory.total_bytes,
                memory_used_bytes: current.memory.used_bytes,
                receive_bytes_per_second: None,
                transmit_bytes_per_second: None,
            }
        };

        self.previous = Some(current);
        Ok(metrics)
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("[bingux-statusd] {error}");
        process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let metrics_listener = bind_socket(METRICS_SOCKET_NAME)?;
    let osd_listener = bind_socket(OSD_SOCKET_NAME)?;
    eprintln!("[bingux-statusd] metrics socket ready");
    eprintln!("[bingux-statusd] OSD socket ready");
    let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
    start_client_listener(
        metrics_listener,
        sender.clone(),
        METRICS_SOCKET_NAME,
        Event::MetricsClient,
    );
    start_client_listener(
        osd_listener,
        sender.clone(),
        OSD_SOCKET_NAME,
        Event::OsdClient,
    );
    gnoblin::start_state_subscriber(sender);

    let mut sampler = Sampler::new();
    let mut latest_metrics = sampler.sample()?;
    let mut desktop_state = DesktopState::default();
    let mut latest_record = record_json(latest_metrics, &desktop_state)?;
    let mut metrics_clients = Vec::new();
    let mut osd_clients = Vec::new();
    let mut next_sample = Instant::now() + SAMPLE_INTERVAL;

    loop {
        if next_sample <= Instant::now() {
            publish_sample(
                &mut sampler,
                &mut latest_metrics,
                &desktop_state,
                &mut latest_record,
                &mut metrics_clients,
            )?;
            next_sample += SAMPLE_INTERVAL;

            if next_sample <= Instant::now() {
                next_sample = Instant::now() + SAMPLE_INTERVAL;
            }

            continue;
        }

        let timeout = next_sample.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(timeout) {
            Ok(Event::MetricsClient(mut client)) => {
                prune_disconnected_clients(&mut metrics_clients);
                if metrics_clients.len() < MAX_CLIENTS
                    && client.set_nonblocking(true).is_ok()
                    && write_record(&mut client, &latest_record)
                {
                    metrics_clients.push(client);
                }
            }
            Ok(Event::OsdClient(client)) => {
                // OSD is transient: a new client receives only later requests.
                prune_disconnected_clients(&mut osd_clients);
                if osd_clients.len() < MAX_CLIENTS && client.set_nonblocking(true).is_ok() {
                    osd_clients.push(client);
                }
            }
            Ok(Event::DesktopState(state)) => {
                desktop_state = state;
                latest_record = record_json(latest_metrics, &desktop_state)?;
                publish_record(&mut metrics_clients, &latest_record);
            }
            Ok(Event::OsdRequest(request)) => {
                let record = osd_json(&request)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                publish_record(&mut osd_clients, &record);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("event source stopped"));
            }
        }
    }
}

fn publish_sample(
    sampler: &mut Sampler,
    latest_metrics: &mut Metrics,
    desktop_state: &DesktopState,
    latest_record: &mut String,
    clients: &mut Vec<UnixStream>,
) -> io::Result<()> {
    *latest_metrics = sampler.sample()?;
    *latest_record = record_json(*latest_metrics, desktop_state)?;
    publish_record(clients, latest_record);
    Ok(())
}

fn publish_record(clients: &mut Vec<UnixStream>, record: &str) {
    clients.retain_mut(|client| write_record(client, record));
}

fn record_json(metrics: Metrics, desktop_state: &DesktopState) -> io::Result<String> {
    metrics_with_desktop_state_json(metrics, desktop_state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn start_client_listener(
    listener: UnixListener,
    sender: mpsc::SyncSender<Event>,
    socket_name: &'static str,
    event: fn(UnixStream) -> Event,
) {
    thread::spawn(move || {
        let mut retry_delay = CLIENT_LISTENER_INITIAL_RETRY_DELAY;

        loop {
            match listener.accept() {
                Ok((stream, _address)) => {
                    retry_delay = CLIENT_LISTENER_INITIAL_RETRY_DELAY;

                    if sender.send(event(stream)).is_err() {
                        return;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    eprintln!("[bingux-statusd] {socket_name} accept failed: {error}");
                    thread::sleep(retry_delay);
                    retry_delay = std::cmp::min(retry_delay * 2, CLIENT_LISTENER_MAX_RETRY_DELAY);
                }
            }
        }
    });
}

fn bind_socket(socket_name: &str) -> io::Result<UnixListener> {
    let runtime_directory = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;

    bind_socket_in(&runtime_directory.join("bingux"), socket_name)
}

fn bind_socket_in(directory: &Path, socket_name: &str) -> io::Result<UnixListener> {
    fs::create_dir_all(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;

    let path = directory.join(socket_name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_socket() => match probe_socket(&path) {
            Ok(()) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("refusing to replace active socket {}", path.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                fs::remove_file(&path)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("refusing to replace active socket {}", path.display()),
                ));
            }
            Err(error) => return Err(error),
        },
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("refusing to replace non-socket path {}", path.display()),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

fn probe_socket(path: &Path) -> io::Result<()> {
    async_io::block_on(async {
        let connect = async_io::Async::<UnixStream>::connect(path);
        let timeout = async_io::Timer::after(ACTIVE_SOCKET_PROBE_TIMEOUT);
        futures_util::pin_mut!(connect, timeout);

        match futures_util::future::select(connect, timeout).await {
            futures_util::future::Either::Left((result, _timeout)) => result.map(|_| ()),
            futures_util::future::Either::Right((_elapsed, _connect)) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "active socket probe timed out",
            )),
        }
    })
}

fn write_record(client: &mut UnixStream, record: &str) -> bool {
    client.write_all(record.as_bytes()).is_ok()
}

fn prune_disconnected_clients(clients: &mut Vec<UnixStream>) {
    let mut probe = [0; 1];
    clients.retain_mut(|client| match client.read(&mut probe) {
        Ok(0) => false,
        Ok(_) => false,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) =>
        {
            true
        }
        Err(_) => false,
    });
}

fn read_cpu_sample() -> io::Result<bingux_statusd::CpuSample> {
    read_proc("/proc/stat", parse_cpu_stat)
}

fn read_memory_sample() -> io::Result<bingux_statusd::MemorySample> {
    read_proc("/proc/meminfo", parse_meminfo)
}

fn read_network_totals() -> io::Result<bingux_statusd::NetworkTotals> {
    read_proc("/proc/net/dev", parse_network_totals)
}

fn read_proc<T>(path: &str, parse: impl FnOnce(&str) -> Result<T, &'static str>) -> io::Result<T> {
    let contents = fs::read_to_string(path)?;
    parse(&contents).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::{bind_socket_in, prune_disconnected_clients};
    use std::{
        env, fs,
        io::ErrorKind,
        os::unix::net::UnixStream,
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestDirectory(PathBuf);

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temporary_socket_directory() -> TestDirectory {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("bingux-statusd-{}-{nonce}", process::id()));
        fs::create_dir(&path).unwrap();
        TestDirectory(path)
    }

    #[test]
    fn prunes_disconnected_clients_before_the_client_cap() {
        let (server, client) = UnixStream::pair().unwrap();
        server.set_nonblocking(true).unwrap();
        drop(client);

        let mut clients = vec![server];
        prune_disconnected_clients(&mut clients);

        assert!(clients.is_empty());
    }

    #[test]
    fn retains_connected_clients_without_pending_data() {
        let (server, _client) = UnixStream::pair().unwrap();
        server.set_nonblocking(true).unwrap();

        let mut clients = vec![server];
        prune_disconnected_clients(&mut clients);

        assert_eq!(clients.len(), 1);
    }

    #[test]
    fn refuses_to_replace_an_active_socket() {
        let directory = temporary_socket_directory();
        let _listener = bind_socket_in(&directory.0, "active.sock").unwrap();

        let error = bind_socket_in(&directory.0, "active.sock").unwrap_err();

        assert_eq!(error.kind(), ErrorKind::AddrInUse);
    }

    #[test]
    fn replaces_a_stale_socket() {
        let directory = temporary_socket_directory();
        let listener = bind_socket_in(&directory.0, "stale.sock").unwrap();
        drop(listener);

        let replacement = bind_socket_in(&directory.0, "stale.sock").unwrap();

        assert!(replacement.local_addr().is_ok());
    }
}
