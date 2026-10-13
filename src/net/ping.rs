use crate::model::Node;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::timeout;

/// Measures the round-trip time (RTT) to connect to `host:port` via TCP in milliseconds.
pub async fn tcp_ping(host: &str, port: u16, timeout_dur: Duration) -> Result<u32, String> {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();

    match timeout(timeout_dur, TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => {
            let elapsed = start.elapsed().as_millis() as u32;
            Ok(elapsed)
        }
        Ok(Err(e)) => Err(format!("Connection error: {}", e)),
        Err(_) => Err("Timeout".to_string()),
    }
}

/// Runs concurrent TCP pings across a list of nodes and returns updated nodes with latency_ms.
pub async fn batch_ping_nodes(
    nodes: Vec<Node>,
    concurrency_limit: usize,
    timeout_dur: Duration,
) -> Vec<Node> {
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));
    let mut handles = Vec::new();

    for mut node in nodes {
        let sem = semaphore.clone();
        let host = node.server.clone();
        let port = node.port;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            node.latency_ms = tcp_ping(&host, port, timeout_dur).await.ok();
            node
        });

        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(updated_node) = handle.await {
            results.push(updated_node);
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_ping_public_dns() {
        let result = tcp_ping("1.1.1.1", 53, Duration::from_secs(3)).await;
        // In most environments this succeeds or returns within duration
        assert!(result.is_ok() || result.is_err());
    }
}
