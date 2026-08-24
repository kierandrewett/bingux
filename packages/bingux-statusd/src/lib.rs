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
        Metrics, byte_rate, cpu_percent, metrics_json, parse_cpu_stat, parse_meminfo,
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
}
