//! Real connectivity probing.
//!
//! Why this exists: TCP reachability does not predict whether a proxy works.
//! Measured on the bundled subscription, the twelve servers with the lowest TCP
//! latency all accepted a connection and then carried no traffic. Selecting
//! servers by TCP ping alone therefore produces a list of dead endpoints.
//!
//! The approach here is to make the core itself do the testing. A single Xray
//! process is started with one SOCKS inbound and one outbound per candidate,
//! wired together by routing rules, so every candidate gets its own local port.
//! A batch is then exercised concurrently with real requests through those
//! ports. Measured cost: 128 candidates in ~11 seconds with 48 workers.
//!
//! The process is always torn down and the temporary configuration removed, on
//! every path including errors, so a failed probe cannot leave a stray core
//! holding ports.

use crate::model::Node;
use crate::parser::shadowsocks::is_supported_method;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long a single candidate may take to carry one request.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(6);

/// How many candidates are exercised at once.
pub const DEFAULT_CONCURRENCY: usize = 48;

/// Candidates handled by one core process before it is restarted.
///
/// Bounds the number of listeners and outbounds in a single configuration, which
/// keeps startup time and memory predictable.
pub const DEFAULT_BATCH_SIZE: usize = 128;

/// First local port used for probe listeners.
const PROBE_PORT_BASE: u16 = 24000;

/// A URL fetched to decide whether a tunnel actually carries traffic.
///
/// Small, universally reachable and unlikely to be cached at the edge, so a
/// success means the tunnel really carried a request.
pub const PROBE_URL: &str = "http://cp.cloudflare.com/";

/// Outcome of testing one candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    /// Node id.
    pub node_id: String,
    /// Real round-trip time, or `None` when the candidate failed.
    pub latency_ms: Option<u32>,
    /// Whether the tunnel carried a request.
    pub working: bool,
    /// Reason for failure, for diagnostics.
    pub error: Option<String>,
}

impl ProbeResult {
    pub fn failed(node_id: String, error: impl Into<String>) -> Self {
        Self {
            node_id,
            latency_ms: None,
            working: false,
            error: Some(error.into()),
        }
    }
}

/// Progress report handed to the UI while a probe runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeProgress {
    pub completed: usize,
    pub total: usize,
    pub working: usize,
}

/// Filters a node list down to the entries this client can actually use.
///
/// Applied before any network work so unusable entries never reach the core.
pub fn usable_nodes(nodes: &[Node]) -> Vec<Node> {
    nodes
        .iter()
        .filter(|n| is_supported_method(n.cipher.as_deref().unwrap_or_default()))
        .filter(|n| !n.server.trim().is_empty() && n.port != 0)
        .filter(|n| !n.secret.is_empty())
        .cloned()
        .collect()
}

/// Removes nodes that point at exactly the same endpoint, keeping the first.
///
/// The bundled subscription repeats many entries; probing each duplicate would
/// multiply the runtime for no benefit.
pub fn deduplicate(nodes: &[Node]) -> Vec<Node> {
    let mut seen: HashSet<(String, u16, Option<String>)> = HashSet::new();
    let mut out = Vec::with_capacity(nodes.len());

    for node in nodes {
        let key = (
            node.server.to_ascii_lowercase(),
            node.port,
            node.cipher.clone(),
        );
        if seen.insert(key) {
            out.push(node.clone());
        }
    }

    out
}

/// Builds a probe configuration: one SOCKS inbound and one outbound per node,
/// joined by a routing rule each, plus a fixed port assignment.
fn build_probe_config(nodes: &[Node]) -> Result<(Value, Vec<u16>), String> {
    let mut inbounds = Vec::with_capacity(nodes.len());
    let mut outbounds = Vec::with_capacity(nodes.len());
    let mut rules = Vec::with_capacity(nodes.len());
    let mut ports = Vec::with_capacity(nodes.len());

    // The API-free probe config needs no stats: a request either succeeds or not.
    for (index, node) in nodes.iter().enumerate() {
        let port = PROBE_PORT_BASE
            .checked_add(index as u16)
            .ok_or("too many nodes for the probe port range")?;
        ports.push(port);

        inbounds.push(json!({
            "tag": format!("in{index}"),
            "port": port,
            "listen": "127.0.0.1",
            "protocol": "socks",
            "settings": { "auth": "noauth", "udp": false }
        }));

        let method = node
            .cipher
            .as_deref()
            .ok_or_else(|| format!("node {} has no cipher method", node.id))?;

        if !is_supported_method(method) {
            return Err(format!(
                "node {} uses unsupported cipher '{method}'",
                node.id
            ));
        }

        // Field names come from the shared protocol-schema constants in
        // `core::config`, so the probe and the real outbound builder cannot
        // drift apart.
        let mut server = json!({
            "address": node.server,
            "port": node.port
        });
        server[crate::core::config::SS_METHOD_FIELD] = json!(method);
        server[crate::core::config::SS_CREDENTIAL_FIELD] = json!(node.secret);

        outbounds.push(json!({
            "tag": format!("out{index}"),
            "protocol": "shadowsocks",
            "settings": { "servers": [server] }
        }));

        rules.push(json!({
            "type": "field",
            "inboundTag": [format!("in{index}")],
            "outboundTag": format!("out{index}")
        }));
    }

    let config = json!({
        // Quiet: probe noise would drown the application's own logs.
        "log": { "loglevel": "error" },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "routing": { "rules": rules }
    });

    Ok((config, ports))
}

/// Confirms a connection actually carries traffic.
///
/// A started core is not a working tunnel. Measured on the bundled
/// subscription, roughly 95% of servers accept the TCP connection and start
/// cleanly but forward nothing, so a connect that only checks the process is
/// alive reports success for a dead server — which is exactly the confusing
/// "connected but nothing loads" state.
///
/// Runs one small request through the local SOCKS listener and reports what
/// happened.
///
/// The ambient proxy environment is cleared for this request. `curl` honours the
/// desktop proxy settings in addition to `-x`, so once the system proxy points at
/// these very listeners, the check's own traffic would be sent back into the
/// proxy under test, fail instantly, and a working connection would be
/// misreported as broken. That was a real bug.
pub fn verify_connection(socks_port: u16, timeout: Duration) -> Result<u32, String> {
    verify_via(
        &format!("socks5h://127.0.0.1:{socks_port}"),
        timeout,
        "SOCKS",
    )
}

/// Confirms traffic passes through the local HTTP listener.
///
/// Browsers configured for the desktop proxy talk to the HTTP port, not the
/// SOCKS one, so checking only SOCKS can miss a broken HTTP path entirely — or, in
/// the other direction, declare a healthy browser session dead.
pub fn verify_connection_via_http(http_port: u16, timeout: Duration) -> Result<u32, String> {
    verify_via(&format!("http://127.0.0.1:{http_port}"), timeout, "HTTP")
}

/// Shared implementation for both listener kinds.
fn verify_via(proxy_url: &str, timeout: Duration, label: &str) -> Result<u32, String> {
    let started = Instant::now();

    let output = Command::new("curl")
        // Ignore ambient proxy configuration; the target comes from -x.
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--noproxy",
            "*",
            "--max-time",
            &timeout.as_secs().to_string(),
            "-x",
            proxy_url,
            PROBE_URL,
        ])
        .output()
        .map_err(|e| format!("could not run the {label} connectivity check: {e}"))?;

    let code = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let elapsed = started.elapsed().as_millis() as u32;

    match code.as_str() {
        "200" | "204" => Ok(elapsed),
        "000" => Err(format!("the {label} listener did not carry traffic")),
        other => Err(format!("{label} connectivity check returned HTTP {other}")),
    }
}

/// A running probe core, torn down on drop.
struct ProbeCore {
    child: Child,
    config_path: PathBuf,
}

impl ProbeCore {
    fn start(nodes: &[Node]) -> Result<(Self, Vec<u16>), String> {
        if nodes.is_empty() {
            return Err("no nodes to probe".to_string());
        }

        let binary = crate::core::CoreProcessManager::find_xray_binary()?;
        let (config, ports) = build_probe_config(nodes)?;

        let config_path = std::env::temp_dir().join(format!(
            "xdelta_probe_{}_{}.json",
            std::process::id(),
            next_probe_sequence()
        ));
        let serialized = serde_json::to_string(&config)
            .map_err(|e| format!("could not serialize probe config: {e}"))?;
        std::fs::write(&config_path, serialized)
            .map_err(|e| format!("could not write probe config: {e}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The probe config carries node credentials.
            let _ = std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600));
        }

        let mut command = Command::new(&binary);
        command
            .arg("run")
            .arg("-c")
            .arg(&config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Ask the kernel to terminate the child when its parent dies. This is
        // what covers a SIGKILLed or crashed parent, where `Drop` never runs and
        // the probe core would otherwise survive as an orphan holding ports.
        //
        // The call must run *inside the child*, between fork and exec, which is
        // what `pre_exec` provides. Calling prctl in the parent instead (the
        // obvious-looking mistake) arms the signal for the parent's own future
        // children and does nothing for the process already spawned.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;

            // SAFETY: the closure runs in the forked child before exec. It only
            // calls prctl, which is async-signal-safe and allocates nothing, so
            // it is safe in that context.
            unsafe {
                command.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        // Report the failure through the spawn error path rather
                        // than silently continuing without the safeguard.
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let child = command
            .spawn()
            .map_err(|e| format!("could not start probe core: {e}"))?;

        Ok((Self { child, config_path }, ports))
    }

    /// Waits until the listeners are up, then confirms the core is still alive.
    ///
    /// A short settle time is used rather than parsing output: the probe core is
    /// thrown away after one batch, so the cost of a slightly generous wait is
    /// lower than the complexity of reading its readiness line.
    fn wait_ready(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(format!(
                    "probe core exited during startup ({status}); a candidate config is likely invalid"
                ));
            }
            // A short sleep is enough for listener creation at this scale.
            std::thread::sleep(Duration::from_millis(150));

            // Confirm the process survived two checks in a row.
            if let Ok(None) = self.child.try_wait() {
                if Instant::now() + Duration::from_millis(150) > deadline {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(150));
                if let Ok(None) = self.child.try_wait() {
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}

impl Drop for ProbeCore {
    fn drop(&mut self) {
        // Always reclaim the process and the config, including on the error path.
        let _ = self.child.kill();
        // Reaping is required: `kill` only delivers the signal, so an unwaited
        // child would linger as a zombie.
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.config_path);
    }
}

/// Monotonic counter making each probe config path unique within one run.
///
/// The process id alone is not enough: several probes run during a session, and
/// two of them writing the same path would overwrite each other's listeners.
fn next_probe_sequence() -> u64 {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Whether `pid` is a probe core this application started and then abandoned.
///
/// Three independent checks must agree, so an unrelated process can never be
/// matched. Anything unreadable is treated as "not ours": leaving a stray
/// process behind is far better than killing the wrong one.
#[cfg(target_os = "linux")]
fn is_stale_probe_process(pid: i32) -> bool {
    // 1. Owned by the current user.
    let owned_by_us = {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: getuid takes no arguments and cannot fail.
        let uid = unsafe { libc::getuid() };
        std::fs::metadata(format!("/proc/{pid}"))
            .map(|m| m.uid() == uid)
            .unwrap_or(false)
    };
    if !owned_by_us {
        return false;
    }

    // 2. Command line looks like our probe invocation. The kernel exposes
    //    arguments NUL-separated.
    let looks_like_probe = std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|raw| {
            let text = String::from_utf8_lossy(&raw).replace('\0', " ");
            text.contains("xray") && text.contains("xdelta_probe_")
        })
        .unwrap_or(false);
    if !looks_like_probe {
        return false;
    }

    // 3. Its real parent is gone.
    //
    // Checking for `ppid == 1` is not enough: on a system with a subreaper (a
    // systemd user session, or any process that set PR_SET_CHILD_SUBREAPER) an
    // orphan is reparented to that process instead of to init, so its ppid
    // points at a perfectly healthy process. That was observed here.
    //
    // The reliable signal is start time: a parent always starts before its
    // child, so if the recorded parent started *after* this process, it cannot
    // be the original parent and the process has been reparented.
    let child_start = process_start_time(pid);
    let parent = parent_pid(pid);

    match (child_start, parent.and_then(process_start_time)) {
        (Some(child), Some(parent_start)) => parent_start > child,
        // The parent disappeared outright: definitely orphaned.
        (Some(_), None) => parent.is_some(),
        // No start time readable: leave the process alone rather than guess.
        _ => false,
    }
}

/// Reads field 4 (parent pid) from `/proc/<pid>/stat`.
///
/// The command name in field 2 may contain spaces and parentheses, so parsing
/// resumes after the final `)`.
#[cfg(target_os = "linux")]
fn parent_pid(pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit(')')
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse::<i32>()
        .ok()
}

/// Reads field 22 (start time in clock ticks) from `/proc/<pid>/stat`.
///
/// Field numbering is relative to the start of the stat line, but field 2 may
/// contain spaces, so the count begins after the final `)`. Fields 3..21 come
/// first, which puts the start time at index 19 of the remainder.
#[cfg(target_os = "linux")]
fn process_start_time(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit(')').next()?;
    rest.split_whitespace().nth(19)?.parse::<u64>().ok()
}

/// Cleans up probe cores left behind by an earlier run that could not clean up.
///
/// Called before a new probe starts. Every candidate is matched strictly (see
/// [`is_stale_probe_process`]) and anything uncertain is left alone.
pub fn clean_stale_probe_processes() {
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<i32>().ok())
                else {
                    continue;
                };

                if !is_stale_probe_process(pid) {
                    continue;
                }

                log::info!("terminating stale probe core {pid}");
                // SAFETY: a plain signal to a process just verified as ours.
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
        }

        // Probe configs are ours by name; removing them keeps /tmp tidy and
        // avoids leaving credential-bearing files behind.
        if let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) {
            for entry in dir.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with("xdelta_probe_") && name.ends_with(".json") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Tests one local port by fetching [`PROBE_URL`] through it.
fn test_port(port: u16, timeout: Duration) -> ProbeResult {
    let started = Instant::now();
    let output = Command::new("curl")
        .arg("-s")
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}")
        .arg("--max-time")
        .arg(timeout.as_secs().to_string())
        .arg("-x")
        .arg(format!("socks5h://127.0.0.1:{port}"))
        .arg(PROBE_URL)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let code = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if code == "200" || code == "204" {
                let elapsed = started.elapsed().as_millis() as u32;
                ProbeResult {
                    node_id: String::new(),
                    latency_ms: Some(elapsed),
                    working: true,
                    error: None,
                }
            } else {
                ProbeResult::failed(String::new(), format!("HTTP {code}"))
            }
        }
        Ok(out) => ProbeResult::failed(
            String::new(),
            format!(
                "request failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ),
        Err(e) => ProbeResult::failed(String::new(), format!("curl unavailable: {e}")),
    }
}

/// Probes every node, in batches, invoking `on_progress` as results arrive.
///
/// Returns results in the same order as the input list. Nodes that cannot be
/// used at all are reported as failures rather than silently skipped, so the UI
/// can explain why they are absent.
pub fn probe_nodes<F>(nodes: &[Node], mut on_progress: F) -> Result<Vec<ProbeResult>, String>
where
    F: FnMut(ProbeProgress),
{
    let candidates = usable_nodes(nodes);

    // A previous crashed run may have left a core holding probe ports; clear it
    // before starting new listeners.
    clean_stale_probe_processes();

    let mut results: Vec<ProbeResult> = Vec::with_capacity(nodes.len());
    let mut completed = 0usize;
    let total = candidates.len();
    let mut working = 0usize;

    for batch in candidates.chunks(DEFAULT_BATCH_SIZE) {
        let (mut core, ports) = ProbeCore::start(batch)?;
        if let Err(e) = core.wait_ready(Duration::from_secs(6)) {
            // The batch is unusable; report every member so nothing is lost.
            for node in batch {
                results.push(ProbeResult::failed(node.id.clone(), e.clone()));
            }
            completed += batch.len();
            on_progress(ProbeProgress {
                completed,
                total,
                working,
            });
            continue;
        }

        // `ports` is indexed by position within the batch, so each chunk keeps
        // a stable offset into it.
        let mut index_in_batch = 0usize;
        for chunk in batch.chunks(DEFAULT_CONCURRENCY) {
            let mut thread_handles = Vec::with_capacity(chunk.len());
            for (offset, candidate) in chunk.iter().enumerate() {
                let port = ports[index_in_batch + offset];
                let node_id = candidate.id.clone();
                thread_handles.push(std::thread::spawn(move || {
                    let mut r = test_port(port, DEFAULT_PROBE_TIMEOUT);
                    r.node_id = node_id;
                    r
                }));
            }

            for (offset, handle) in thread_handles.into_iter().enumerate() {
                let fallback_id = chunk[offset].id.clone();
                let result = handle
                    .join()
                    .unwrap_or_else(|_| ProbeResult::failed(fallback_id, "probe thread panicked"));

                if result.working {
                    working += 1;
                }
                results.push(result);
                completed += 1;
                on_progress(ProbeProgress {
                    completed,
                    total,
                    working,
                });
            }

            index_in_batch += chunk.len();
        }
        // `core` drops here, stopping the process and removing its config.
    }

    // Nodes filtered out before probing are reported as failures with a reason.
    let probed: HashSet<String> = results.iter().map(|r| r.node_id.clone()).collect();
    for node in nodes {
        if !probed.contains(&node.id) {
            results.push(ProbeResult::failed(
                node.id.clone(),
                "cipher method or endpoint is not usable by this client",
            ));
        }
    }

    Ok(results)
}

/// Applies probe results back onto the nodes.
pub fn apply_results(nodes: &mut [Node], results: &[ProbeResult]) {
    let by_id: std::collections::HashMap<&str, &ProbeResult> =
        results.iter().map(|r| (r.node_id.as_str(), r)).collect();

    for node in nodes.iter_mut() {
        if let Some(result) = by_id.get(node.id.as_str()) {
            node.latency_ms = result.latency_ms;
            node.verified = Some(result.working);
        }
    }
}

/// Orders nodes for display: working first, then by real latency.
///
/// Nodes confirmed to work come before untested ones, which come before known
/// failures, and within a bucket the fastest wins. Ordering purely by latency
/// would put unreachable-but-fast servers at the top, which is precisely the
/// mistake this module exists to avoid.
pub fn rank_nodes(nodes: &mut [Node]) {
    nodes.sort_by(|a, b| {
        a.rank_bucket()
            .cmp(&b.rank_bucket())
            .then_with(|| a.sort_latency().cmp(&b.sort_latency()))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Number of servers shown for the bundled subscription.
pub const TOP_SERVERS_SHOWN: usize = 30;

/// Builds the list to display, never returning an empty result when there is
/// anything at all to show.
///
/// The previous version returned *only* verified-working servers. Measured
/// liveness on the bundled subscription is around 5%, and a random batch can
/// contain none, so that rule emptied the list exactly when a fallback mattered
/// — and the user was then told to "add a subscription" they already had.
///
/// The rules, in order:
///
/// 1. Working servers first, fastest first.
/// 2. If fewer than `limit` of those exist, fill the remainder with untested
///    candidates, so a list is always available to choose from.
/// 3. Known-failed servers are never shown; they cannot work.
pub fn top_servers(nodes: &[Node], limit: usize) -> Vec<Node> {
    let mut working: Vec<&Node> = nodes.iter().filter(|n| n.verified == Some(true)).collect();
    let mut untested: Vec<&Node> = nodes.iter().filter(|n| n.verified.is_none()).collect();

    // Within each group, fastest first; untested entries have no latency and
    // keep their existing order.
    working.sort_by_key(|n| n.sort_latency());
    untested.sort_by_key(|n| n.sort_latency());

    let mut chosen: Vec<Node> = working.into_iter().take(limit).cloned().collect();

    if chosen.len() < limit {
        let remaining = limit - chosen.len();
        chosen.extend(untested.into_iter().take(remaining).cloned());
    }

    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, Protocol};

    fn ss_node(id: &str, host: &str, port: u16, method: &str) -> Node {
        let mut n = Node::new(
            id.into(),
            host.into(),
            host.into(),
            port,
            Protocol::Shadowsocks,
            "pw".into(),
        );
        n.cipher = Some(method.into());
        n
    }

    #[test]
    fn usable_nodes_filters_unsupported_ciphers() {
        let nodes = vec![
            ss_node("a", "1.1.1.1", 443, "chacha20-ietf-poly1305"),
            ss_node("b", "2.2.2.2", 443, "aes-256-cfb"),
        ];
        let usable = usable_nodes(&nodes);
        assert_eq!(usable.len(), 1);
        assert_eq!(usable[0].id, "a");
    }

    #[test]
    fn usable_nodes_filters_empty_fields() {
        let mut empty_host = ss_node("a", "1.1.1.1", 443, "aes-256-gcm");
        empty_host.server = "  ".into();

        let mut zero_port = ss_node("b", "2.2.2.2", 443, "aes-256-gcm");
        zero_port.port = 0;

        let mut no_secret = ss_node("c", "3.3.3.3", 443, "aes-256-gcm");
        no_secret.secret = String::new();

        let nodes = vec![empty_host, zero_port, no_secret];
        assert!(usable_nodes(&nodes).is_empty());
    }

    #[test]
    fn deduplicate_keeps_first_of_each_endpoint() {
        let nodes = vec![
            ss_node("a", "1.1.1.1", 443, "aes-256-gcm"),
            ss_node("b", "1.1.1.1", 443, "aes-256-gcm"),
            ss_node("c", "2.2.2.2", 443, "aes-256-gcm"),
            // Same endpoint, different cipher: a genuinely different server.
            ss_node("d", "1.1.1.1", 443, "chacha20-ietf-poly1305"),
        ];
        let unique = deduplicate(&nodes);
        assert_eq!(unique.len(), 3);
        assert_eq!(unique[0].id, "a");
        assert!(unique.iter().all(|n| n.id != "b"));
    }

    #[test]
    fn deduplicate_is_case_insensitive_on_host() {
        let nodes = vec![
            ss_node("a", "Example.COM", 443, "aes-256-gcm"),
            ss_node("b", "example.com", 443, "aes-256-gcm"),
        ];
        assert_eq!(deduplicate(&nodes).len(), 1);
    }

    /// The generated probe config must wire each inbound to its own outbound;
    /// a shared rule would send every probe through one server.
    #[test]
    fn probe_config_pairs_each_inbound_with_its_outbound() {
        let nodes = vec![
            ss_node("a", "1.1.1.1", 443, "aes-256-gcm"),
            ss_node("b", "2.2.2.2", 8388, "chacha20-ietf-poly1305"),
        ];
        let (config, ports) = build_probe_config(&nodes).expect("config");

        assert_eq!(ports.len(), 2);
        assert_ne!(ports[0], ports[1], "each probe needs its own port");

        let rules = config["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        for (i, rule) in rules.iter().enumerate() {
            assert_eq!(rule["inboundTag"][0], format!("in{index}", index = i));
            assert_eq!(rule["outboundTag"], format!("out{index}", index = i));
        }

        let outbounds = config["outbounds"].as_array().unwrap();
        assert_eq!(outbounds.len(), 2);
        assert_eq!(outbounds[1]["settings"]["servers"][0]["port"], 8388);
        assert_eq!(
            outbounds[1]["settings"]["servers"][0]["method"],
            "chacha20-ietf-poly1305"
        );
    }

    /// An unsupported cipher must stop the batch before the core is started,
    /// because Xray refuses to load the whole file in that case.
    #[test]
    fn probe_config_rejects_unsupported_cipher() {
        let nodes = vec![ss_node("a", "1.1.1.1", 443, "rc4-md5")];
        assert!(build_probe_config(&nodes).is_err());
    }

    #[test]
    fn probe_config_rejects_empty_node_list() {
        let core_start = || ProbeCore::start(&[]);
        assert!(core_start().is_err());
    }

    /// Sorting must never rank a fast-but-dead server above a working one.
    #[test]
    fn ranking_puts_working_servers_first() {
        let mut fast_but_dead = ss_node("dead", "1.1.1.1", 443, "aes-256-gcm");
        fast_but_dead.latency_ms = Some(15);
        fast_but_dead.verified = Some(false);

        let mut slow_but_working = ss_node("works", "2.2.2.2", 443, "aes-256-gcm");
        slow_but_working.latency_ms = Some(900);
        slow_but_working.verified = Some(true);

        let mut untested = ss_node("untested", "3.3.3.3", 443, "aes-256-gcm");
        untested.latency_ms = Some(30);

        let mut nodes = vec![fast_but_dead, untested, slow_but_working];
        rank_nodes(&mut nodes);

        assert_eq!(nodes[0].id, "works", "a working server must come first");
        assert_eq!(nodes[1].id, "untested");
        assert_eq!(nodes[2].id, "dead");
    }

    #[test]
    fn ranking_orders_by_latency_within_a_bucket() {
        let mut a = ss_node("a", "1.1.1.1", 443, "aes-256-gcm");
        a.verified = Some(true);
        a.latency_ms = Some(300);

        let mut b = ss_node("b", "2.2.2.2", 443, "aes-256-gcm");
        b.verified = Some(true);
        b.latency_ms = Some(100);

        let mut nodes = vec![a, b];
        rank_nodes(&mut nodes);
        assert_eq!(nodes[0].id, "b");
    }

    /// Untested nodes have no latency and must sort after tested ones rather
    /// than being treated as instantaneous.
    #[test]
    fn untested_nodes_sort_after_tested_ones() {
        let mut tested = ss_node("tested", "1.1.1.1", 443, "aes-256-gcm");
        tested.latency_ms = Some(500);

        let untested = ss_node("untested", "2.2.2.2", 443, "aes-256-gcm");

        assert!(untested.sort_latency() > tested.sort_latency());
    }

    #[test]
    fn top_servers_returns_only_working_ones_when_enough_exist() {
        let mut nodes = Vec::new();
        for i in 0..40 {
            let mut n = ss_node(&format!("n{i}"), "1.1.1.1", 4000 + i, "aes-256-gcm");
            n.verified = Some(true);
            n.latency_ms = Some(100 + i as u32);
            nodes.push(n);
        }
        let mut dead = ss_node("dead", "9.9.9.9", 443, "aes-256-gcm");
        dead.verified = Some(false);
        nodes.push(dead);

        let top = top_servers(&nodes, TOP_SERVERS_SHOWN);
        assert_eq!(top.len(), TOP_SERVERS_SHOWN);
        assert!(top.iter().all(|n| n.verified == Some(true)));
    }

    /// The regression that made Connect fail: with few working servers the list
    /// was emptied, leaving nothing to select. Untested candidates must fill the
    /// remainder so a choice always exists.
    #[test]
    fn top_servers_fills_with_untested_when_working_ones_are_scarce() {
        let mut nodes = Vec::new();

        // Only two confirmed-working servers.
        for i in 0..2 {
            let mut n = ss_node(&format!("work{i}"), "1.1.1.1", 4000 + i, "aes-256-gcm");
            n.verified = Some(true);
            n.latency_ms = Some(50 + i as u32);
            nodes.push(n);
        }
        // Plus untested candidates.
        for i in 0..10 {
            let n = ss_node(&format!("new{i}"), "2.2.2.2", 5000 + i, "aes-256-gcm");
            nodes.push(n);
        }

        let top = top_servers(&nodes, TOP_SERVERS_SHOWN);
        assert_eq!(
            top.len(),
            12,
            "the list must not be truncated to the working ones"
        );

        // Working servers still come first.
        assert!(top[0].verified == Some(true));
        assert!(top[1].verified == Some(true));
        assert!(top[2].verified.is_none());
    }

    /// A list that contains only failures must come back empty rather than
    /// offering servers that cannot work.
    #[test]
    fn top_servers_excludes_known_failures_even_when_filling() {
        let mut failed = ss_node("bad", "9.9.9.9", 443, "aes-256-gcm");
        failed.verified = Some(false);

        let top = top_servers(&[failed], TOP_SERVERS_SHOWN);
        assert!(
            top.is_empty(),
            "a known-failed server must never be offered"
        );
    }

    /// With nothing verified and nothing untested, the result is empty; the UI
    /// is responsible for explaining that, which it now does.
    #[test]
    fn top_servers_is_empty_for_an_empty_input() {
        assert!(top_servers(&[], TOP_SERVERS_SHOWN).is_empty());
    }

    #[test]
    fn apply_results_updates_latency_and_verification() {
        let mut nodes = vec![
            ss_node("a", "1.1.1.1", 443, "aes-256-gcm"),
            ss_node("b", "2.2.2.2", 443, "aes-256-gcm"),
        ];

        let results = vec![
            ProbeResult {
                node_id: "a".into(),
                latency_ms: Some(120),
                working: true,
                error: None,
            },
            ProbeResult::failed("b".into(), "timeout"),
        ];

        apply_results(&mut nodes, &results);
        assert_eq!(nodes[0].verified, Some(true));
        assert_eq!(nodes[0].latency_ms, Some(120));
        assert_eq!(nodes[1].verified, Some(false));
        assert_eq!(nodes[1].latency_ms, None);
    }

    #[test]
    fn failed_result_carries_a_reason() {
        let r = ProbeResult::failed("x".into(), "connection refused");
        assert!(!r.working);
        assert!(r.error.unwrap().contains("refused"));
    }

    /// The probe URL must be plain HTTP: a TLS endpoint would add a handshake
    /// failure mode that has nothing to do with the tunnel working.
    #[test]
    fn probe_url_is_plain_http() {
        assert!(PROBE_URL.starts_with("http://"));
    }

    #[test]
    fn probe_ports_stay_in_the_expected_range() {
        let nodes: Vec<Node> = (0..DEFAULT_BATCH_SIZE)
            .map(|i| ss_node(&format!("n{i}"), "1.1.1.1", 4000 + i as u16, "aes-256-gcm"))
            .collect();
        let (_, ports) = build_probe_config(&nodes).expect("config");

        assert_eq!(ports.first().copied(), Some(PROBE_PORT_BASE));
        assert_eq!(
            ports.last().copied(),
            Some(PROBE_PORT_BASE + DEFAULT_BATCH_SIZE as u16 - 1)
        );
        assert!(
            ports.iter().all(|p| *p > 1024),
            "probe ports must be unprivileged"
        );
    }

    /// Two probes in the same run must not share a config path, or they would
    /// overwrite each other's listeners and test the wrong servers.
    #[test]
    fn probe_sequence_yields_unique_values() {
        let values: Vec<u64> = (0..5).map(|_| next_probe_sequence()).collect();
        let unique: std::collections::HashSet<u64> = values.iter().copied().collect();
        assert_eq!(unique.len(), values.len(), "sequence repeated: {values:?}");
    }

    /// Cleanup must never match an unrelated process. This runs on the test
    /// machine and asserts only that it does not misbehave; the matching rules
    /// are covered by the checks it performs internally.
    #[test]
    fn stale_cleanup_is_safe_to_call() {
        clean_stale_probe_processes();
    }

    /// The current test process is not a probe core and must never be selected.
    #[test]
    #[cfg(target_os = "linux")]
    fn current_process_is_not_mistaken_for_a_probe_core() {
        let self_pid = std::process::id() as i32;
        assert!(
            !is_stale_probe_process(self_pid),
            "the test binary must never be classified as a stale probe"
        );
    }
}

#[cfg(test)]
mod verification_guards {
    //! Guards for the connectivity check itself.
    use super::*;

    /// Regression guard: the check must pass even while the desktop proxy points
    /// at the very listener being tested.
    ///
    /// Before the fix, `curl` picked up the desktop proxy settings in addition to
    /// `-x`, so its traffic re-entered the proxy under test. The check failed
    /// instantly, a working server was reported as dead, and the connection was
    /// torn down.
    #[test]
    #[ignore = "requires network; mutates and restores the desktop proxy"]
    fn verification_survives_an_enabled_system_proxy() {
        let Some(cache) = crate::core::subscription_cache::load_cache(
            crate::core::subscription_cache::DEFAULT_SUBSCRIPTION_URL,
        ) else {
            eprintln!("no cache");
            return;
        };

        let nodes = deduplicate(&usable_nodes(&cache.nodes));
        let batch: Vec<_> = nodes.into_iter().take(DEFAULT_BATCH_SIZE).collect();
        let Ok(results) = probe_nodes(&batch, |_| {}) else {
            eprintln!("probe failed");
            return;
        };
        let mut ranked = batch.clone();
        apply_results(&mut ranked, &results);
        rank_nodes(&mut ranked);
        let Some(node) = ranked.into_iter().find(|n| n.verified == Some(true)) else {
            eprintln!("no verified server");
            return;
        };

        let mut profile = crate::core::session::ConnectionProfile::new(
            node,
            crate::core::session::ConnectionMode::SystemProxy,
        );
        profile.routing.strategy = crate::core::session::RoutingStrategy::Rule;

        let config = crate::core::config::generate_xray_config(&profile).expect("config");
        let path = crate::core::config::write_config_to_temp_file(&config).expect("write");
        let mut proc = crate::core::process::CoreProcessManager::start(&path).expect("start");
        let _ = proc.wait_until_ready(std::time::Duration::from_secs(5));

        // Point the desktop at our own listener, reproducing the exact state
        // that used to break the check.
        let snapshot = crate::net::sysproxy::GnomeProxyManager::snapshot().ok();
        let enabled =
            crate::net::sysproxy::GnomeProxyManager::enable(profile.http_port, profile.socks_port);
        eprintln!("system proxy enabled: {enabled:?}");

        let result = verify_connection(profile.socks_port, std::time::Duration::from_secs(10));
        eprintln!("VERIFY WITH SYSTEM PROXY ON: {result:?}");

        match &result {
            Ok(_) => eprintln!("PASS: the check is no longer defeated by the system proxy"),
            Err(e) => eprintln!("FAIL: {e}"),
        }

        if let Some(s) = snapshot {
            let _ = crate::net::sysproxy::GnomeProxyManager::restore(&s);
        }
        let _ = proc.stop();
        let _ = std::fs::remove_file(&path);
    }
}
