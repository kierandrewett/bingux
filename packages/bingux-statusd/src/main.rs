use bingux_statusd::{
    Metrics, byte_rate, cpu_percent, metrics_json, parse_cpu_stat, parse_meminfo,
    parse_network_totals,
};
use std::{
    env, fs,
    io::{self, Write},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    process,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const SOCKET_NAME: &str = "metrics-v1.sock";
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const MAX_CLIENTS: usize = 16;

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
    let listener = bind_socket()?;
    eprintln!("[bingux-statusd] metrics socket ready");
    let (sender, receiver) = mpsc::sync_channel(MAX_CLIENTS);
    thread::spawn(move || {
        for connection in listener.incoming() {
            match connection {
                Ok(stream) => {
                    if sender.send(stream).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    eprintln!("[bingux-statusd] socket accept failed: {error}");
                    return;
                }
            }
        }
    });

    let mut sampler = Sampler::new();
    let mut latest_record = metrics_json(sampler.sample()?);
    let mut clients = Vec::new();
    let mut next_sample = Instant::now() + SAMPLE_INTERVAL;

    loop {
        if next_sample <= Instant::now() {
            publish_sample(&mut sampler, &mut latest_record, &mut clients)?;
            next_sample += SAMPLE_INTERVAL;

            if next_sample <= Instant::now() {
                next_sample = Instant::now() + SAMPLE_INTERVAL;
            }

            continue;
        }

        let timeout = next_sample.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(timeout) {
            Ok(mut client) => {
                if clients.len() < MAX_CLIENTS
                    && client.set_nonblocking(true).is_ok()
                    && write_record(&mut client, &latest_record)
                {
                    clients.push(client);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("socket accept thread stopped"));
            }
        }
    }
}

fn publish_sample(
    sampler: &mut Sampler,
    latest_record: &mut String,
    clients: &mut Vec<UnixStream>,
) -> io::Result<()> {
    *latest_record = metrics_json(sampler.sample()?);
    clients.retain_mut(|client| write_record(client, latest_record));
    Ok(())
}

fn bind_socket() -> io::Result<UnixListener> {
    let runtime_directory = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;
    let directory = runtime_directory.join("bingux");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;

    let path = directory.join(SOCKET_NAME);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(&path)?,
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

fn write_record(client: &mut UnixStream, record: &str) -> bool {
    client.write_all(record.as_bytes()).is_ok()
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
