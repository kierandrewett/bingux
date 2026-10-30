//! Optional Linux metrics. Missing sensors must not interrupt the core feed.
use crate::byte_rate;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraMetrics {
    pub sampled_at_ms: u64,
    pub hardware: crate::hardware::Hardware,
    pub cpu_cores: Vec<CoreUsage>,
    pub cpu_temperature_celsius: Option<f64>,
    pub load1: Option<f64>,
    pub load5: Option<f64>,
    pub load15: Option<f64>,
    pub logical_cpus: Option<usize>,
    pub swap_used_bytes: Option<u64>,
    pub swap_total_bytes: Option<u64>,
    pub disk_read_bytes_per_second: Option<f64>,
    pub disk_write_bytes_per_second: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreUsage {
    pub id: u32,
    pub usage: Option<f64>,
}

fn core_counters(input: &str) -> BTreeMap<u32, crate::CpuSample> {
    input
        .lines()
        .filter_map(|line| {
            let (label, counters) = line.split_once(' ')?;
            let id = label.strip_prefix("cpu")?.parse::<u32>().ok()?;
            let sample = crate::parse_cpu_stat(&format!("cpu {counters}")).ok()?;
            Some((id, sample))
        })
        .take(256)
        .collect()
}

fn core_usage(
    previous: &BTreeMap<u32, crate::CpuSample>,
    current: &BTreeMap<u32, crate::CpuSample>,
) -> Vec<CoreUsage> {
    current
        .iter()
        .map(|(&id, &sample)| CoreUsage {
            id,
            usage: previous
                .get(&id)
                .and_then(|&before| crate::cpu_percent(before, sample)),
        })
        .collect()
}

type Disks = BTreeMap<String, (u64, u64)>;
#[derive(Default)]
pub struct Sampler {
    hardware: crate::hardware::Sampler,
    cores: BTreeMap<u32, crate::CpuSample>,
    previous: Option<(Instant, Disks)>,
}
impl Sampler {
    pub fn sample(&mut self) -> ExtraMetrics {
        let mut result = ExtraMetrics {
            sampled_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            hardware: self.hardware.sample(),
            cpu_temperature_celsius: cpu_temperature(Path::new("/sys/class/hwmon")),
            logical_cpus: std::thread::available_parallelism().ok().map(usize::from),
            ..ExtraMetrics::default()
        };
        let cores = fs::read_to_string("/proc/stat")
            .map(|text| core_counters(&text))
            .unwrap_or_default();
        result.cpu_cores = core_usage(&self.cores, &cores);
        self.cores = cores;
        if let Ok(text) = fs::read_to_string("/proc/loadavg") {
            [result.load1, result.load5, result.load15] = parse_load(&text);
        }
        if let Ok(text) = fs::read_to_string("/proc/meminfo") {
            (result.swap_used_bytes, result.swap_total_bytes) = parse_swap(&text);
        }
        match fs::read_to_string("/proc/diskstats") {
            Ok(text) => {
                let current = parse_disks(&text, Path::new("/sys/block"));
                let now = Instant::now();
                if let Some((at, previous)) = &self.previous {
                    (
                        result.disk_read_bytes_per_second,
                        result.disk_write_bytes_per_second,
                    ) = disk_rates(previous, &current, now.duration_since(*at));
                }
                self.previous = Some((now, current));
            }
            Err(_) => self.previous = None,
        }
        result
    }
}

pub fn parse_load(text: &str) -> [Option<f64>; 3] {
    let mut fields = text.split_whitespace();
    std::array::from_fn(|_| {
        fields
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
    })
}

pub fn parse_swap(text: &str) -> (Option<u64>, Option<u64>) {
    fn bytes(text: &str, key: &str) -> Option<u64> {
        text.lines()
            .find_map(|line| line.strip_prefix(key))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    }
    let total = bytes(text, "SwapTotal:");
    let used = total.and_then(|t| bytes(text, "SwapFree:").and_then(|free| t.checked_sub(free)));
    (used, total)
}

fn parse_disks(text: &str, sys_block: &Path) -> Disks {
    text.lines()
        .filter_map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() < 10 || !sys_block.join(f[2]).join("device").exists() {
                return None;
            }
            // Kernel diskstats sector counters always use 512-byte sectors.
            let read = f[5].parse::<u64>().ok()?.checked_mul(512)?;
            let written = f[9].parse::<u64>().ok()?.checked_mul(512)?;
            Some((format!("{}:{}:{}", f[0], f[1], f[2]), (read, written)))
        })
        .collect()
}

fn disk_rates(previous: &Disks, current: &Disks, elapsed: Duration) -> (Option<f64>, Option<f64>) {
    let mut read = 0.0;
    let mut written = 0.0;
    let mut count = 0;
    for (device, (r, w)) in current {
        if let Some((pr, pw)) = previous.get(device) {
            if let (Some(rd), Some(wd)) = (byte_rate(*pr, *r, elapsed), byte_rate(*pw, *w, elapsed))
            {
                read += rd;
                written += wd;
                count += 1;
            }
        }
    }
    if count > 0 {
        (Some(read), Some(written))
    } else {
        (None, None)
    }
}

fn cpu_temperature(root: &Path) -> Option<f64> {
    let mut values = Vec::new();
    for directory in fs::read_dir(root).ok()?.flatten() {
        let path = directory.path();
        let name = fs::read_to_string(path.join("name")).unwrap_or_default();
        // Generic motherboard and GPU sensors cannot reliably identify CPU temperature.
        if !matches!(
            name.trim(),
            "coretemp" | "k10temp" | "zenpower" | "cpu_thermal"
        ) {
            continue;
        }
        for file in fs::read_dir(path).ok().into_iter().flatten().flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if name.starts_with("temp") && name.ends_with("_input") {
                if let Some(value) = fs::read_to_string(file.path())
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|v| v / 1000.0)
                    .filter(|v| v.is_finite() && (0.0..=150.0).contains(v))
                {
                    values.push(value);
                }
            }
        }
    }
    values.into_iter().reduce(f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn swap_handles_zero_missing_and_invalid_counters() {
        assert_eq!(
            parse_swap("SwapTotal: 20 kB\nSwapFree: 8 kB"),
            (Some(12288), Some(20480))
        );
        assert_eq!(
            parse_swap("SwapTotal: 0 kB\nSwapFree: 0 kB"),
            (Some(0), Some(0))
        );
        assert_eq!(parse_swap("SwapTotal: 1 kB\nSwapFree: 2 kB").0, None);
        assert_eq!(parse_swap(""), (None, None));
        assert_eq!(parse_load("1.2 NaN -1 1/4"), [Some(1.2), None, None]);
    }
    #[test]
    fn disk_rates_ignore_new_devices_and_counter_resets() {
        let previous = BTreeMap::from([("sda".into(), (1024, 2048))]);
        let current = BTreeMap::from([
            ("sda".into(), (3072, 6144)),
            ("sdb".into(), (999999, 999999)),
        ]);
        assert_eq!(
            disk_rates(&previous, &current, Duration::from_secs(2)),
            (Some(1024.0), Some(2048.0))
        );
        assert_eq!(
            disk_rates(&current, &previous, Duration::from_secs(1)),
            (None, None)
        );
    }
    #[test]
    fn hardware_selection_avoids_partitions_and_non_cpu_temperatures() {
        let root = std::env::temp_dir().join(format!("bingux-extra-{}", std::process::id()));
        fs::create_dir_all(root.join("block/sda/device")).unwrap();
        let disks = parse_disks(
            "8 0 sda 1 0 10 0 1 0 20 0\n8 1 sda1 1 0 10 0 1 0 20 0\n253 0 dm-0 1 0 10 0 1 0 20 0",
            &root.join("block"),
        );
        assert_eq!(disks.len(), 1);
        assert_eq!(disks.values().next(), Some(&(5120, 10240)));
        for (name, driver, temperature) in
            [("hwmon0", "nvme", "90000"), ("hwmon1", "coretemp", "54000")]
        {
            let directory = root.join("hwmon").join(name);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("name"), driver).unwrap();
            fs::write(directory.join("temp1_input"), temperature).unwrap();
        }
        assert_eq!(cpu_temperature(&root.join("hwmon")), Some(54.0));
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod core_tests {
    use super::*;
    #[test]
    fn independent_cores_and_hotplug() {
        let first =
            core_counters("cpu 5 0 0 5 0 0 0 0\ncpu0 5 0 0 5 0 0 0 0\ncpu3 0 0 0 10 0 0 0 0");
        let next =
            core_counters("cpu0 15 0 0 5 0 0 0 0\ncpu3 0 0 0 20 0 0 0 0\ncpu8 1 0 0 1 0 0 0 0");
        let usage = core_usage(&first, &next);
        assert_eq!(
            usage.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![0, 3, 8]
        );
        assert_eq!(usage[0].usage, Some(100.0));
        assert_eq!(usage[1].usage, Some(0.0));
        assert_eq!(usage[2].usage, None);
        assert_eq!(core_usage(&next, &first)[0].usage, None);
    }
}
