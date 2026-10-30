//! Read-only hardware and process inventory. Optional kernel fields stay null.
use serde::Serialize;
use std::{collections::BTreeMap, fs, path::Path, time::Instant};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub cpu_model: String,
    pub cpu_mhz: Option<f64>,
    pub uptime_seconds: Option<f64>,
    pub kernel: String,
    pub gpus: Vec<Gpu>,
    pub storage: Vec<Storage>,
    pub processes_sampled_at_ms: u64,
    pub process_count: usize,
    pub running_processes: usize,
    pub threads: u64,
    pub processes: Vec<Process>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub name: String,
    pub driver: String,
    pub pci_address: String,
    pub busy_percent: Option<f64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub temperature_celsius: Option<f64>,
    pub power_watts: Option<f64>,
    pub clock_mhz: Option<f64>,
    pub fan_rpm: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
    pub path: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub executable: String,
    pub argv: Vec<String>,
    pub start_time: u64,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: u64,
    pub threads: u64,
    pub state: String,
}
#[derive(Debug)]
struct Counters {
    name: String,
    ticks: u64,
    start: u64,
    rss: u64,
    threads: u64,
    state: String,
}
fn parse_process(text: &str) -> Option<Counters> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let fields: Vec<_> = text.get(close + 2..)?.split_whitespace().collect();
    Some(Counters {
        name: text[open + 1..close]
            .chars()
            .filter(|c| !c.is_control())
            .take(80)
            .collect(),
        ticks: fields
            .get(11)?
            .parse::<u64>()
            .ok()?
            .checked_add(fields.get(12)?.parse().ok()?)?,
        start: fields.get(19)?.parse().ok()?,
        rss: fields.get(21)?.parse().ok()?,
        threads: fields.get(17)?.parse().ok()?,
        state: fields.first()?.to_string(),
    })
}
fn parse_argv(bytes: &[u8]) -> Vec<String> {
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes
        .strip_suffix(&[0])
        .unwrap_or(bytes)
        .split(|byte| *byte == 0)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect()
}
#[derive(Default)]
pub struct Sampler {
    previous: BTreeMap<u32, (u64, u64)>,
    at: Option<Instant>,
    process_snapshot: Hardware,
}
impl Sampler {
    pub fn sample(&mut self) -> Hardware {
        let now = Instant::now();
        let elapsed = self.at.map(|at| now.duration_since(at).as_secs_f64());
        // sysconf has no pointer arguments and returns -1 for unsupported keys.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as u64;
        let mut current = BTreeMap::new();
        let mut processes = Vec::new();
        let mut result = self.process_snapshot.clone();
        if self
            .at
            .is_none_or(|at| now.duration_since(at).as_secs() >= 6)
        {
            result = Hardware::default();
            for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<u32>().ok())
                else {
                    continue;
                };
                let Some(c) = fs::read_to_string(entry.path().join("stat"))
                    .ok()
                    .and_then(|s| parse_process(&s))
                else {
                    continue;
                };
                let cpu = self
                    .previous
                    .get(&pid)
                    .filter(|(start, _)| *start == c.start)
                    .and_then(|(_, ticks)| c.ticks.checked_sub(*ticks))
                    .zip(elapsed)
                    .filter(|(_, seconds)| *seconds > 0.0)
                    .map(|(ticks, seconds)| ticks as f64 / hz / seconds * 100.0);
                current.insert(pid, (c.start, c.ticks));
                result.running_processes += usize::from(c.state == "R");
                result.threads += c.threads;
                processes.push(Process {
                    pid,
                    name: c.name,
                    executable: String::new(),
                    argv: Vec::new(),
                    start_time: c.start,
                    cpu_percent: cpu,
                    memory_bytes: c.rss.saturating_mul(page),
                    threads: c.threads,
                    state: c.state,
                });
            }
            result.process_count = processes.len();
            self.previous = current;
            self.at = Some(now);
            // Bound pathological process counts while retaining the complete normal desktop list.
            processes.sort_by_key(|process| process.pid);
            result.processes = processes
                .into_iter()
                .take(8192)
                .map(|mut process| {
                    process.executable = fs::read_link(format!("/proc/{}/exe", process.pid))
                        .ok()
                        .and_then(|p| {
                            p.file_name()
                                .map(|s| s.to_string_lossy().chars().take(256).collect())
                        })
                        .unwrap_or_default();
                    process.argv = fs::read(format!("/proc/{}/cmdline", process.pid))
                        .map(|bytes| parse_argv(&bytes))
                        .unwrap_or_default();
                    process
                })
                .collect();
            result.processes_sampled_at_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            self.process_snapshot = result.clone();
        }
        let cpu = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        result.cpu_model = cpu
            .lines()
            .find_map(|s| {
                s.strip_prefix("model name")
                    .and_then(|s| s.split_once(':'))
                    .map(|(_, s)| s.trim().to_string())
            })
            .unwrap_or_default();
        let frequencies: Vec<f64> = cpu
            .lines()
            .filter_map(|s| {
                s.strip_prefix("cpu MHz")
                    .and_then(|s| s.split_once(':'))
                    .and_then(|(_, s)| s.trim().parse().ok())
            })
            .collect();
        if !frequencies.is_empty() {
            result.cpu_mhz = Some(frequencies.iter().sum::<f64>() / frequencies.len() as f64);
        }
        result.uptime_seconds = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse().ok());
        result.kernel = read(Path::new("/proc/sys/kernel/osrelease")).unwrap_or_default();
        result.gpus = gpus(Path::new("/sys/class/drm"));
        for path in [Some("/".to_string()), std::env::var("HOME").ok()]
            .into_iter()
            .flatten()
        {
            if let Some(storage) = storage(&path) {
                result.storage.push(storage);
            }
        }
        result
    }
}
fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().chars().take(256).collect())
}
fn number(path: &Path) -> Option<u64> {
    read(path)?.parse().ok()
}
fn gpu_name(device: &Path) -> String {
    if let Some(name) = read(&device.join("product_name")) {
        return name;
    }
    let vendor = read(&device.join("vendor"))
        .unwrap_or_default()
        .trim_start_matches("0x")
        .to_string();
    let id = read(&device.join("device"))
        .unwrap_or_default()
        .trim_start_matches("0x")
        .to_string();
    static IDS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| {
        ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"]
            .iter()
            .find_map(|p| fs::read_to_string(p).ok())
            .unwrap_or_default()
    });
    let mut active = false;
    for line in ids.lines() {
        if !line.starts_with(['\t', '#']) && !line.is_empty() {
            active = line.starts_with(&format!("{vendor} "));
        }
        if active && line.starts_with(&format!("\t{id} ")) {
            return line[5..].trim().to_string();
        }
    }
    format!(
        "{} GPU ({id})",
        match vendor.as_str() {
            "1002" => "AMD",
            "10de" => "NVIDIA",
            "8086" => "Intel",
            _ => "PCI",
        }
    )
}
fn gpus(root: &Path) -> Vec<Gpu> {
    let mut result = Vec::new();
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name
            .strip_prefix("card")
            .is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        let device = entry.path().join("device");
        if !device.exists() {
            continue;
        }
        let mut gpu = Gpu {
            name: gpu_name(&device),
            driver: fs::read_link(device.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
                .unwrap_or_default(),
            pci_address: fs::canonicalize(&device)
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
                .unwrap_or(name),
            busy_percent: number(&device.join("gpu_busy_percent"))
                .filter(|v| *v <= 100)
                .map(|v| v as f64),
            memory_used_bytes: number(&device.join("mem_info_vram_used")),
            memory_total_bytes: number(&device.join("mem_info_vram_total")),
            ..Gpu::default()
        };
        for sensor in fs::read_dir(device.join("hwmon"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let p = sensor.path();
            gpu.temperature_celsius = number(&p.join("temp1_input")).map(|v| v as f64 / 1000.0);
            gpu.power_watts = number(&p.join("power1_average"))
                .or_else(|| number(&p.join("power1_input")))
                .map(|v| v as f64 / 1e6);
            gpu.clock_mhz = number(&p.join("freq1_input")).map(|v| v as f64 / 1e6);
            gpu.fan_rpm = number(&p.join("fan1_input")).map(|v| v as f64);
        }
        result.push(gpu);
        if result.len() == 8 {
            break;
        }
    }
    result.sort_by(|a, b| a.pci_address.cmp(&b.pci_address));
    result
}
fn storage(path: &str) -> Option<Storage> {
    let cpath = std::ffi::CString::new(path).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // statvfs initializes the output on success; the CString remains alive.
    if unsafe { libc::statvfs(cpath.as_ptr(), info.as_mut_ptr()) } != 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some(Storage {
        path: path.into(),
        total_bytes: info.f_blocks.saturating_mul(info.f_frsize),
        available_bytes: info.f_bavail.saturating_mul(info.f_frsize),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_preserves_boundaries_and_empty_arguments() {
        assert_eq!(
            parse_argv(b"/bin/example\0two words\0\0'quoted'\0"),
            vec!["/bin/example", "two words", "", "'quoted'"]
        );
        assert!(parse_argv(b"").is_empty());
        assert_eq!(parse_argv(b"example"), vec!["example"]);
    }
    #[test]
    fn names_with_parentheses_and_spaces() {
        let mut fields = vec!["0"; 22];
        fields[0] = "R";
        fields[11] = "20";
        fields[12] = "5";
        fields[17] = "4";
        fields[19] = "123";
        fields[21] = "8";
        let c = parse_process(&format!("42 (a ) process) {}", fields.join(" "))).unwrap();
        assert_eq!(c.name, "a ) process");
        assert_eq!(c.ticks, 25);
        assert_eq!(c.start, 123);
        assert_eq!(c.rss, 8);
        assert!(parse_process("broken").is_none());
    }
    #[test]
    fn optional_gpu_sensors_and_connector_filtering() {
        let root = std::env::temp_dir().join(format!("bingux-gpu-{}", std::process::id()));
        let device = root.join("card0/device");
        fs::create_dir_all(&device).unwrap();
        fs::create_dir_all(root.join("card0-HDMI-A-1")).unwrap();
        fs::write(device.join("product_name"), "Test GPU").unwrap();
        fs::write(device.join("gpu_busy_percent"), "42").unwrap();
        let data = gpus(&root);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].busy_percent, Some(42.0));
        assert_eq!(data[0].temperature_celsius, None);
        fs::remove_dir_all(root).unwrap();
    }
}
