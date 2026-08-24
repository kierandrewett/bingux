use serde::Serialize;
use std::time::Duration;

/// CPU tick counters from the aggregate `cpu` line in `/proc/stat`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuSample {
    pub total: u64,
    pub idle: u64,
}

/// Memory capacity and used memory calculated from `/proc/meminfo`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemorySample {
    pub total_bytes: u64,
    pub used_bytes: u64,
}

/// Aggregate non-loopback byte counters from `/proc/net/dev`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkTotals {
    pub receive_bytes: u64,
    pub transmit_bytes: u64,
}

/// The complete metrics record sent to each Bingux desktop-shell client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub cpu_percent: Option<f64>,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub receive_bytes_per_second: Option<f64>,
    pub transmit_bytes_per_second: Option<f64>,
}

/// The only OSD record version accepted from Gnoblin and sent to shell clients.
pub const OSD_PROTOCOL_VERSION: u32 = 2;

// These byte limits keep a fully JSON-escaped OSD record well below Quickshell's
// 64 KiB line limit.
const MAX_OSD_ICON_BYTES: usize = 256;
const MAX_OSD_LABEL_BYTES: usize = 2_048;
const MAX_OSD_OUTPUT_NAME_BYTES: usize = 128;
const MAX_OSD_OUTPUT_NAMES: usize = 16;
const MAX_OSD_OUTPUT_NAME_BYTES_TOTAL: usize = 1_024;

/// OSD records are published only on the separate osd-v2 socket so existing
/// metrics-v1 clients never receive an unexpected record type.
#[derive(Clone, Debug, PartialEq)]
pub struct OsdRequest {
    monitor_index: i32,
    output_names: Vec<String>,
    icon: String,
    label: String,
    level: f64,
    max_level: f64,
}

impl OsdRequest {
    /// Construct an OSD request only when it stays within the shell socket boundary.
    pub fn new(
        monitor_index: i32,
        output_names: Vec<String>,
        icon: String,
        label: String,
        level: f64,
        max_level: f64,
    ) -> Option<Self> {
        if monitor_index < 0
            || !output_names_are_valid(&output_names)
            || !level.is_finite()
            || level < -1.0
            || !max_level.is_finite()
            || max_level < -1.0
            || icon.len() > MAX_OSD_ICON_BYTES
            || label.len() > MAX_OSD_LABEL_BYTES
            || contains_control_characters(&icon)
            || contains_control_characters(&label)
        {
            return None;
        }

        Some(Self {
            monitor_index,
            output_names,
            icon,
            label,
            level,
            max_level,
        })
    }
}

fn output_names_are_valid(output_names: &[String]) -> bool {
    if output_names.is_empty()
        || output_names.len() > MAX_OSD_OUTPUT_NAMES
        || output_names.iter().map(String::len).sum::<usize>() > MAX_OSD_OUTPUT_NAME_BYTES_TOTAL
    {
        return false;
    }

    output_names.iter().enumerate().all(|(index, output_name)| {
        !output_name.is_empty()
            && output_name.len() <= MAX_OSD_OUTPUT_NAME_BYTES
            && !contains_control_characters(output_name)
            && !output_names[..index].contains(output_name)
    })
}

fn contains_control_characters(value: &str) -> bool {
    value.chars().any(char::is_control)
}

#[derive(Serialize)]
struct OsdRecord<'a> {
    #[serde(rename = "protocolVersion")]
    protocol_version: u32,
    #[serde(rename = "type")]
    record_type: &'static str,
    #[serde(rename = "monitorIndex")]
    monitor_index: i32,
    #[serde(rename = "outputNames")]
    output_names: &'a [String],
    icon: &'a str,
    label: &'a str,
    level: f64,
    #[serde(rename = "maxLevel")]
    max_level: f64,
}

/// Encode a newline-delimited JSON OSD v2 record for the transient shell socket.
pub fn osd_json(request: &OsdRequest) -> Result<String, serde_json::Error> {
    serde_json::to_string(&OsdRecord {
        protocol_version: OSD_PROTOCOL_VERSION,
        record_type: "osd",
        monitor_index: request.monitor_index,
        output_names: &request.output_names,
        icon: &request.icon,
        label: &request.label,
        level: request.level,
        max_level: request.max_level,
    })
    .map(|record| format!("{record}\n"))
}

/// One GNOME Shell input source exposed by the Gnoblin control interface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InputSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub id: String,
    #[serde(rename = "shortName")]
    pub short_name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

/// The three privacy states exposed by the Gnoblin control interface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrivacyState {
    pub screen_sharing: bool,
    pub microphone_in_use: bool,
    pub location_in_use: bool,
}

/// Desktop state that supplements the sampled local system metrics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DesktopState {
    pub available: bool,
    pub input_sources: Vec<InputSource>,
    pub current_input_source: Option<InputSource>,
    pub privacy: PrivacyState,
}

/// Encode a newline-delimited JSON metrics record for the local shell socket.
pub fn metrics_json(metrics: Metrics) -> String {
    format!(
        "{{\"protocolVersion\":1,\"type\":\"metrics\",\"cpuPercent\":{},\"memoryTotalBytes\":{},\"memoryUsedBytes\":{},\"networkReceiveBytesPerSecond\":{},\"networkTransmitBytesPerSecond\":{}}}\n",
        format_optional_number(metrics.cpu_percent),
        metrics.memory_total_bytes,
        metrics.memory_used_bytes,
        format_optional_number(metrics.receive_bytes_per_second),
        format_optional_number(metrics.transmit_bytes_per_second),
    )
}

/// Encode metrics and GNOME Shell state in one newline-delimited socket record.
///
/// This uses a separate serializer for the text-bearing state so an input
/// source name cannot corrupt the JSON stream.
pub fn metrics_with_desktop_state_json(
    metrics: Metrics,
    desktop_state: &DesktopState,
) -> Result<String, serde_json::Error> {
    let input_sources = serde_json::to_string(&desktop_state.input_sources)?;
    let current_input_source = serde_json::to_string(&desktop_state.current_input_source)?;

    Ok(format!(
        "{{\"protocolVersion\":1,\"type\":\"metrics\",\"cpuPercent\":{},\"memoryTotalBytes\":{},\"memoryUsedBytes\":{},\"networkReceiveBytesPerSecond\":{},\"networkTransmitBytesPerSecond\":{},\"desktopStateAvailable\":{},\"inputSources\":{},\"currentInputSource\":{},\"screenSharing\":{},\"microphoneInUse\":{},\"locationInUse\":{}}}\n",
        format_optional_number(metrics.cpu_percent),
        metrics.memory_total_bytes,
        metrics.memory_used_bytes,
        format_optional_number(metrics.receive_bytes_per_second),
        format_optional_number(metrics.transmit_bytes_per_second),
        desktop_state.available,
        input_sources,
        current_input_source,
        desktop_state.privacy.screen_sharing,
        desktop_state.privacy.microphone_in_use,
        desktop_state.privacy.location_in_use,
    ))
}

/// Parse one aggregate CPU accounting sample from `/proc/stat`.
pub fn parse_cpu_stat(input: &str) -> Result<CpuSample, &'static str> {
    let line = input
        .lines()
        .find(|line| line.starts_with("cpu "))
        .ok_or("missing aggregate cpu line")?;
    let mut fields = line.split_whitespace().skip(1);
    let user = parse_u64(fields.next())?;
    let nice = parse_u64(fields.next())?;
    let system = parse_u64(fields.next())?;
    let idle = parse_u64(fields.next())?;
    let iowait = parse_u64(fields.next())?;
    let irq = parse_u64(fields.next())?;
    let softirq = parse_u64(fields.next())?;
    let steal = parse_u64(fields.next())?;
    let total = user
        .checked_add(nice)
        .and_then(|value| value.checked_add(system))
        .and_then(|value| value.checked_add(idle))
        .and_then(|value| value.checked_add(iowait))
        .and_then(|value| value.checked_add(irq))
        .and_then(|value| value.checked_add(softirq))
        .and_then(|value| value.checked_add(steal))
        .ok_or("cpu tick counter overflow")?;

    Ok(CpuSample {
        total,
        idle: idle
            .checked_add(iowait)
            .ok_or("idle cpu tick counter overflow")?,
    })
}

/// Calculate busy CPU percentage between two cumulative CPU samples.
pub fn cpu_percent(previous: CpuSample, current: CpuSample) -> Option<f64> {
    let total_delta = current.total.checked_sub(previous.total)?;
    let idle_delta = current.idle.checked_sub(previous.idle)?;

    if total_delta == 0 {
        return None;
    }

    let busy_delta = total_delta.saturating_sub(idle_delta);
    Some((busy_delta as f64 * 100.0 / total_delta as f64).clamp(0.0, 100.0))
}

/// Parse total and available memory from `/proc/meminfo`.
pub fn parse_meminfo(input: &str) -> Result<MemorySample, &'static str> {
    let mut total_kib = None;
    let mut available_kib = None;

    for line in input.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let amount = match name {
            "MemTotal" | "MemAvailable" => parse_u64(value.split_whitespace().next())?,
            _ => continue,
        };

        match name {
            "MemTotal" => total_kib = Some(amount),
            "MemAvailable" => available_kib = Some(amount),
            _ => {}
        }
    }

    let total_bytes = total_kib
        .ok_or("missing MemTotal")?
        .checked_mul(1024)
        .ok_or("memory total overflow")?;
    let available_bytes = available_kib
        .ok_or("missing MemAvailable")?
        .checked_mul(1024)
        .ok_or("memory available overflow")?;

    Ok(MemorySample {
        total_bytes,
        used_bytes: total_bytes
            .checked_sub(available_bytes)
            .ok_or("available memory exceeds total memory")?,
    })
}

/// Parse aggregate non-loopback byte counters from `/proc/net/dev`.
pub fn parse_network_totals(input: &str) -> Result<NetworkTotals, &'static str> {
    let mut receive_bytes = 0_u64;
    let mut transmit_bytes = 0_u64;

    for line in input.lines() {
        let Some((interface, counters)) = line.split_once(':') else {
            continue;
        };

        if interface.trim() == "lo" {
            continue;
        }

        let mut fields = counters.split_whitespace();
        let received = parse_u64(fields.next())?;

        for _ in 0..7 {
            fields.next().ok_or("missing network receive counter")?;
        }

        let transmitted = parse_u64(fields.next())?;
        receive_bytes = receive_bytes
            .checked_add(received)
            .ok_or("network receive counter overflow")?;
        transmit_bytes = transmit_bytes
            .checked_add(transmitted)
            .ok_or("network transmit counter overflow")?;
    }

    Ok(NetworkTotals {
        receive_bytes,
        transmit_bytes,
    })
}

/// Calculate a byte-per-second rate from two cumulative counters.
pub fn byte_rate(previous: u64, current: u64, elapsed: Duration) -> Option<f64> {
    let elapsed_seconds = elapsed.as_secs_f64();

    if elapsed_seconds == 0.0 {
        return None;
    }

    Some(current.checked_sub(previous)? as f64 / elapsed_seconds)
}

fn format_optional_number(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_owned(), |number| format!("{number:.2}"))
}

fn parse_u64(value: Option<&str>) -> Result<u64, &'static str> {
    value
        .ok_or("missing counter")?
        .parse()
        .map_err(|_| "invalid counter")
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopState, InputSource, Metrics, OsdRequest, PrivacyState, byte_rate, cpu_percent,
        metrics_json, metrics_with_desktop_state_json, osd_json, parse_cpu_stat, parse_meminfo,
        parse_network_totals,
    };

    #[test]
    fn calculates_cpu_usage_from_total_and_idle_deltas() {
        let previous = parse_cpu_stat("cpu  100 0 100 800 0 0 0 0 0 0\n").unwrap();
        let current = parse_cpu_stat("cpu  200 0 150 850 0 0 0 0 0 0\n").unwrap();

        assert_eq!(cpu_percent(previous, current), Some(75.0));
    }

    #[test]
    fn includes_iowait_in_the_idle_cpu_total() {
        let sample = parse_cpu_stat("cpu  10 0 10 70 10 0 0 0 0 0\n").unwrap();

        assert_eq!(sample.total, 100);
        assert_eq!(sample.idle, 80);
    }

    #[test]
    fn converts_meminfo_kib_to_bytes() {
        let memory = parse_meminfo("MemTotal:       16384 kB\nMemAvailable:    4096 kB\n").unwrap();

        assert_eq!(memory.total_bytes, 16_777_216);
        assert_eq!(memory.used_bytes, 12_582_912);
    }

    #[test]
    fn excludes_loopback_traffic_from_network_totals() {
        let network = parse_network_totals(
            "Inter-|   Receive                                                |  Transmit\n\
             face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
                lo: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0\n\
              enp5s0: 3000 0 0 0 0 0 0 0 4000 0 0 0 0 0 0 0\n",
        )
        .unwrap();

        assert_eq!(network.receive_bytes, 3000);
        assert_eq!(network.transmit_bytes, 4000);
    }

    #[test]
    fn calculates_network_byte_rates_from_counter_deltas() {
        assert_eq!(
            byte_rate(1_000, 3_000, std::time::Duration::from_secs(2)),
            Some(1_000.0),
        );
    }

    #[test]
    fn serialises_a_complete_metrics_record_for_the_shell() {
        let record = metrics_json(Metrics {
            cpu_percent: Some(12.5),
            memory_total_bytes: 16_777_216,
            memory_used_bytes: 12_582_912,
            receive_bytes_per_second: Some(1_234.0),
            transmit_bytes_per_second: None,
        });

        assert_eq!(
            record,
            "{\"protocolVersion\":1,\"type\":\"metrics\",\"cpuPercent\":12.50,\"memoryTotalBytes\":16777216,\"memoryUsedBytes\":12582912,\"networkReceiveBytesPerSecond\":1234.00,\"networkTransmitBytesPerSecond\":null}\n",
        );
    }

    #[test]
    fn rejects_non_finite_osd_values() {
        assert!(
            OsdRequest::new(
                0,
                output_names(),
                String::new(),
                String::new(),
                f64::NAN,
                1.0,
            )
            .is_none()
        );
        assert!(
            OsdRequest::new(
                0,
                output_names(),
                String::new(),
                String::new(),
                0.5,
                f64::INFINITY,
            )
            .is_none()
        );
    }

    #[test]
    fn rejects_osd_values_outside_the_shell_range() {
        assert!(
            OsdRequest::new(-1, output_names(), String::new(), String::new(), 0.5, 1.0,).is_none()
        );
        assert!(
            OsdRequest::new(0, output_names(), String::new(), String::new(), -1.01, 1.0,).is_none()
        );
        assert!(
            OsdRequest::new(0, output_names(), String::new(), String::new(), 0.5, -1.01,).is_none()
        );
    }

    #[test]
    fn rejects_osd_text_with_control_characters() {
        assert!(
            OsdRequest::new(
                0,
                output_names(),
                "audio\nvolume".to_owned(),
                String::new(),
                0.5,
                1.0,
            )
            .is_none()
        );
        assert!(
            OsdRequest::new(
                0,
                output_names(),
                String::new(),
                "Volume\u{0000}".to_owned(),
                0.5,
                1.0,
            )
            .is_none()
        );
    }

    #[test]
    fn rejects_invalid_osd_output_names() {
        assert!(OsdRequest::new(0, vec![], String::new(), String::new(), 0.5, 1.0).is_none());
        assert!(
            OsdRequest::new(
                0,
                vec![String::new()],
                String::new(),
                String::new(),
                0.5,
                1.0
            )
            .is_none()
        );
        assert!(
            OsdRequest::new(
                0,
                vec!["DP-1".to_owned(), "DP-1".to_owned()],
                String::new(),
                String::new(),
                0.5,
                1.0,
            )
            .is_none()
        );
        assert!(
            OsdRequest::new(
                0,
                vec!["DP-\n1".to_owned()],
                String::new(),
                String::new(),
                0.5,
                1.0,
            )
            .is_none()
        );
        assert!(
            OsdRequest::new(
                0,
                vec!["é".repeat(65)],
                String::new(),
                String::new(),
                0.5,
                1.0,
            )
            .is_none()
        );
        assert!(
            OsdRequest::new(
                0,
                (0..17).map(|index| format!("DP-{index}")).collect(),
                String::new(),
                String::new(),
                0.5,
                1.0,
            )
            .is_none()
        );

        assert!(
            OsdRequest::new(
                0,
                (0..9)
                    .map(|index| format!("{index:03}-{}", "D".repeat(124)))
                    .collect(),
                String::new(),
                String::new(),
                0.5,
                1.0,
            )
            .is_none()
        );
    }

    #[test]
    fn accepts_empty_osd_text() {
        assert!(
            OsdRequest::new(0, output_names(), String::new(), String::new(), 0.5, 1.0).is_some()
        );
    }

    #[test]
    fn rejects_osd_text_over_the_utf8_byte_limit() {
        assert!(
            OsdRequest::new(0, output_names(), "é".repeat(129), String::new(), 0.5, 1.0).is_none()
        );
        assert!(
            OsdRequest::new(
                0,
                output_names(),
                String::new(),
                "é".repeat(1_025),
                0.5,
                1.0
            )
            .is_none()
        );
    }

    #[test]
    fn serialises_a_transient_osd_record_for_the_shell() {
        let request = OsdRequest::new(
            0,
            output_names(),
            "audio-volume-high-symbolic".to_owned(),
            String::new(),
            0.75,
            1.0,
        )
        .unwrap();

        assert_eq!(
            osd_json(&request).unwrap(),
            "{\"protocolVersion\":2,\"type\":\"osd\",\"monitorIndex\":0,\"outputNames\":[\"DP-1\"],\"icon\":\"audio-volume-high-symbolic\",\"label\":\"\",\"level\":0.75,\"maxLevel\":1.0}\n",
        );
    }

    #[test]
    fn serialises_gnoblin_state_with_a_metrics_record() {
        let record = metrics_with_desktop_state_json(
            Metrics {
                cpu_percent: Some(12.5),
                memory_total_bytes: 16_777_216,
                memory_used_bytes: 12_582_912,
                receive_bytes_per_second: Some(1_234.0),
                transmit_bytes_per_second: None,
            },
            &DesktopState {
                available: true,
                input_sources: vec![InputSource {
                    source_type: "xkb".to_owned(),
                    id: "gb".to_owned(),
                    short_name: "en".to_owned(),
                    display_name: "English \"United Kingdom\"".to_owned(),
                }],
                current_input_source: Some(InputSource {
                    source_type: "xkb".to_owned(),
                    id: "gb".to_owned(),
                    short_name: "en".to_owned(),
                    display_name: "English \"United Kingdom\"".to_owned(),
                }),
                privacy: PrivacyState {
                    screen_sharing: true,
                    microphone_in_use: false,
                    location_in_use: true,
                },
            },
        )
        .unwrap();

        assert_eq!(
            record,
            "{\"protocolVersion\":1,\"type\":\"metrics\",\"cpuPercent\":12.50,\"memoryTotalBytes\":16777216,\"memoryUsedBytes\":12582912,\"networkReceiveBytesPerSecond\":1234.00,\"networkTransmitBytesPerSecond\":null,\"desktopStateAvailable\":true,\"inputSources\":[{\"type\":\"xkb\",\"id\":\"gb\",\"shortName\":\"en\",\"displayName\":\"English \\\"United Kingdom\\\"\"}],\"currentInputSource\":{\"type\":\"xkb\",\"id\":\"gb\",\"shortName\":\"en\",\"displayName\":\"English \\\"United Kingdom\\\"\"},\"screenSharing\":true,\"microphoneInUse\":false,\"locationInUse\":true}\n",
        );
    }

    fn output_names() -> Vec<String> {
        vec!["DP-1".to_owned()]
    }
}
