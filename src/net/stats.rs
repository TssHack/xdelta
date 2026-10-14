//! Real traffic statistics read from the Xray stats API.
//!
//! The dashboard previously showed fixed placeholder values. This module polls
//! the core's `StatsService` over the API inbound and derives live rates from
//! the difference between two samples.
//!
//! The API is queried over HTTP/2 gRPC in the core's own protocol, which is not
//! worth reimplementing here. The supported and stable path is the bundled
//! `xray api statsquery` command, which prints JSON. Using it keeps the data
//! real without a second protocol implementation that could silently drift from
//! the core's own version.

use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};

/// Counter names for the proxied outbound's traffic.
pub const OUTBOUND_UPLINK: &str = "outbound>>>proxy>>>traffic>>>uplink";
pub const OUTBOUND_DOWNLINK: &str = "outbound>>>proxy>>>traffic>>>downlink";

/// Shape of `xray api statsquery` output.
#[derive(Debug, Deserialize)]
struct StatsResponse {
    #[serde(default)]
    stat: Vec<StatEntry>,
}

#[derive(Debug, Deserialize)]
struct StatEntry {
    name: String,
    #[serde(default)]
    value: Option<serde_json::Value>,
}

/// A byte counter value, read as a key/value pair from the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrafficCounters {
    pub uplink_bytes: u64,
    pub downlink_bytes: u64,
}

/// Bytes per second for each direction.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TrafficRates {
    pub downlink_bytes_per_sec: f64,
    pub uplink_bytes_per_sec: f64,
}

/// Parses the JSON printed by `xray api statsquery`.
///
/// Values are delivered as JSON numbers, but a counter that has never been
/// touched comes back without a `value` field at all, so absent entries are
/// treated as zero.
pub fn parse_stats_json(json: &str) -> Result<TrafficCounters, String> {
    let parsed: StatsResponse =
        serde_json::from_str(json).map_err(|e| format!("invalid stats payload: {e}"))?;

    let mut counters = TrafficCounters::default();
    for entry in parsed.stat {
        let value = match entry.value {
            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
            Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0),
            _ => 0,
        };

        match entry.name.as_str() {
            OUTBOUND_UPLINK => counters.uplink_bytes = value,
            OUTBOUND_DOWNLINK => counters.downlink_bytes = value,
            _ => {}
        }
    }

    Ok(counters)
}

/// Parses a flat `name -> value` payload, used by the single-counter query.
pub fn parse_single_stat_json(json: &str) -> Result<u64, String> {
    #[derive(Deserialize)]
    struct Single {
        stat: Option<SingleEntry>,
    }
    #[derive(Deserialize)]
    struct SingleEntry {
        #[serde(default)]
        value: Option<serde_json::Value>,
    }

    let parsed: Single =
        serde_json::from_str(json).map_err(|e| format!("invalid stat payload: {e}"))?;

    Ok(match parsed.stat.and_then(|s| s.value) {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    })
}

/// Queries the core for current traffic counters.
pub fn query_counters(api_port: u16) -> Result<TrafficCounters, String> {
    let binary = crate::core::process::CoreProcessManager::find_xray_binary()
        .unwrap_or_else(|_| std::path::PathBuf::from("xray"));

    let output = Command::new(binary)
        .args([
            "api",
            "statsquery",
            &format!("--server=127.0.0.1:{api_port}"),
        ])
        .output()
        .map_err(|e| format!("could not run xray api: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "stats query failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    parse_stats_json(&String::from_utf8_lossy(&output.stdout))
}

/// Converts two counter samples into per-second rates.
///
/// Counters are monotonic, but the core restarts on every reconnect and resets
/// them, so a decrease is treated as a reset rather than as a huge negative
/// rate.
pub fn rates_between(
    previous: TrafficCounters,
    current: TrafficCounters,
    elapsed: Duration,
) -> TrafficRates {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return TrafficRates::default();
    }

    let delta = |now: u64, before: u64| -> f64 {
        if now >= before {
            (now - before) as f64
        } else {
            // Counter reset: report the current value as new traffic.
            now as f64
        }
    };

    TrafficRates {
        downlink_bytes_per_sec: delta(current.downlink_bytes, previous.downlink_bytes) / seconds,
        uplink_bytes_per_sec: delta(current.uplink_bytes, previous.uplink_bytes) / seconds,
    }
}

/// Formats a byte-per-second rate the way the dashboard displays it.
pub fn format_rate(bytes_per_sec: f64) -> String {
    let value = if bytes_per_sec.is_finite() && bytes_per_sec > 0.0 {
        bytes_per_sec
    } else {
        0.0
    };

    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if value >= GB {
        format!("{:.2} GB/s", value / GB)
    } else if value >= MB {
        format!("{:.2} MB/s", value / MB)
    } else if value >= KB {
        format!("{:.2} KB/s", value / KB)
    } else {
        format!("{:.0} B/s", value)
    }
}

/// Formats a cumulative byte total for the session summary line.
pub fn format_total(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= MB {
        format!("{:.2} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Tracks counters between polls so the dashboard can show live rates.
#[derive(Debug, Default)]
pub struct TrafficSampler {
    last_counters: Option<TrafficCounters>,
    last_instant: Option<Instant>,
}

impl TrafficSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a new sample and returns the rates since the previous one.
    ///
    /// The first sample produces zero rates: there is nothing to compare with
    /// yet, and reporting the whole accumulated traffic as one instant would
    /// spike the display.
    pub fn update(&mut self, counters: TrafficCounters) -> TrafficRates {
        let now = Instant::now();

        let rates = match (self.last_counters, self.last_instant) {
            (Some(previous), Some(then)) => rates_between(previous, counters, now - then),
            _ => TrafficRates::default(),
        };

        self.last_counters = Some(counters);
        self.last_instant = Some(now);
        rates
    }

    /// Total transferred since the sampler was created.
    pub fn totals(&self) -> TrafficCounters {
        self.last_counters.unwrap_or_default()
    }

    /// Drops history, so a new connection does not inherit the old counters.
    pub fn reset(&mut self) {
        self.last_counters = None;
        self.last_instant = None;
    }
}

/// Maps counter names to values, for callers that want both directions at once.
pub fn counters_as_map(counters: TrafficCounters) -> HashMap<&'static str, u64> {
    HashMap::from([
        (OUTBOUND_UPLINK, counters.uplink_bytes),
        (OUTBOUND_DOWNLINK, counters.downlink_bytes),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_directions() {
        let json = r#"{"stat":[
            {"name":"outbound>>>proxy>>>traffic>>>downlink","value":2048},
            {"name":"outbound>>>proxy>>>traffic>>>uplink","value":512},
            {"name":"inbound>>>socks-in>>>traffic>>>uplink","value":9999}
        ]}"#;

        let counters = parse_stats_json(json).expect("parse");
        assert_eq!(counters.downlink_bytes, 2048);
        assert_eq!(counters.uplink_bytes, 512);
        assert_eq!(counters.uplink_bytes, 512, "unrelated counters are ignored");
    }

    /// A counter that has never been touched has no `value` key at all; that
    /// must read as zero, not as a parse failure.
    #[test]
    fn missing_value_is_zero() {
        let json = r#"{"stat":[{"name":"outbound>>>proxy>>>traffic>>>downlink"}]}"#;
        let counters = parse_stats_json(json).expect("parse");
        assert_eq!(counters.downlink_bytes, 0);
    }

    #[test]
    fn empty_stat_list_is_zero() {
        let counters = parse_stats_json(r#"{"stat":[]}"#).expect("parse");
        assert_eq!(counters, TrafficCounters::default());
    }

    #[test]
    fn malformed_json_is_reported() {
        assert!(parse_stats_json("not json").is_err());
    }

    #[test]
    fn single_stat_parses_number_and_string() {
        assert_eq!(
            parse_single_stat_json(r#"{"stat":{"name":"x","value":42}}"#).unwrap(),
            42
        );
        assert_eq!(
            parse_single_stat_json(r#"{"stat":{"name":"x","value":"77"}}"#).unwrap(),
            77
        );
        assert_eq!(parse_single_stat_json(r#"{"stat":{}}"#).unwrap(), 0);
    }

    #[test]
    fn rates_are_per_second() {
        let previous = TrafficCounters {
            uplink_bytes: 0,
            downlink_bytes: 0,
        };
        let current = TrafficCounters {
            uplink_bytes: 500,
            downlink_bytes: 2048,
        };

        let rates = rates_between(previous, current, Duration::from_secs(2));
        assert!((rates.downlink_bytes_per_sec - 1024.0).abs() < 0.001);
        assert!((rates.uplink_bytes_per_sec - 250.0).abs() < 0.001);
    }

    /// A core restart resets the counters. Treating that as a negative delta
    /// would underflow; it must instead count the new value as fresh traffic.
    #[test]
    fn counter_reset_does_not_underflow() {
        let previous = TrafficCounters {
            uplink_bytes: 10_000,
            downlink_bytes: 20_000,
        };
        let current = TrafficCounters {
            uplink_bytes: 100,
            downlink_bytes: 300,
        };

        let rates = rates_between(previous, current, Duration::from_secs(1));
        assert_eq!(rates.uplink_bytes_per_sec, 100.0);
        assert_eq!(rates.downlink_bytes_per_sec, 300.0);
    }

    #[test]
    fn zero_elapsed_time_yields_zero_rates() {
        let rates = rates_between(
            TrafficCounters::default(),
            TrafficCounters {
                uplink_bytes: 1000,
                downlink_bytes: 1000,
            },
            Duration::ZERO,
        );
        assert_eq!(rates, TrafficRates::default());
    }

    /// The first sample has nothing to compare against and must not spike.
    #[test]
    fn first_sample_reports_zero_rates() {
        let mut sampler = TrafficSampler::new();
        let rates = sampler.update(TrafficCounters {
            uplink_bytes: 1_000_000,
            downlink_bytes: 5_000_000,
        });
        assert_eq!(rates, TrafficRates::default());
    }

    #[test]
    fn sampler_tracks_totals_and_resets() {
        let mut sampler = TrafficSampler::new();
        sampler.update(TrafficCounters {
            uplink_bytes: 10,
            downlink_bytes: 20,
        });
        assert_eq!(sampler.totals().downlink_bytes, 20);

        sampler.reset();
        assert_eq!(sampler.totals(), TrafficCounters::default());
    }

    /// A second sample after a real interval must produce a rate.
    #[test]
    fn sampler_produces_a_rate_on_second_sample() {
        let mut sampler = TrafficSampler::new();
        sampler.update(TrafficCounters::default());
        std::thread::sleep(Duration::from_millis(30));

        let rates = sampler.update(TrafficCounters {
            uplink_bytes: 0,
            downlink_bytes: 30_000,
        });
        assert!(
            rates.downlink_bytes_per_sec > 0.0,
            "expected a positive rate, got {}",
            rates.downlink_bytes_per_sec
        );
    }

    #[test]
    fn rate_formatting_uses_readable_units() {
        assert_eq!(format_rate(0.0), "0 B/s");
        assert_eq!(format_rate(-5.0), "0 B/s");
        assert_eq!(format_rate(f64::NAN), "0 B/s");
        assert_eq!(format_rate(512.0), "512 B/s");
        assert_eq!(format_rate(2048.0), "2.00 KB/s");
        assert_eq!(format_rate(1024.0 * 1024.0 * 3.5), "3.50 MB/s");
        assert_eq!(format_rate(1024.0 * 1024.0 * 1024.0 * 2.0), "2.00 GB/s");
    }

    #[test]
    fn total_formatting_uses_readable_units() {
        assert_eq!(format_total(0), "0 B");
        assert_eq!(format_total(512), "512 B");
        assert_eq!(format_total(2048), "2.0 KB");
        assert_eq!(format_total(1024 * 1024 * 3), "3.00 MB");
        assert_eq!(format_total(1024 * 1024 * 1024 * 2), "2.00 GB");
    }

    #[test]
    fn counters_map_exposes_both_directions() {
        let map = counters_as_map(TrafficCounters {
            uplink_bytes: 1,
            downlink_bytes: 2,
        });
        assert_eq!(map.get(OUTBOUND_UPLINK), Some(&1));
        assert_eq!(map.get(OUTBOUND_DOWNLINK), Some(&2));
    }
}
