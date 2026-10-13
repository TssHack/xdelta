use crate::model::{Node, Subscription, SubscriptionUserinfo};
use crate::parser::uri::parse_node_uri;
use base64::prelude::*;
use chrono::{DateTime, Utc};
use std::time::Duration;
use uuid::Uuid;

pub async fn fetch_and_parse_subscription(
    url: &str,
    name: Option<String>,
) -> Result<(Subscription, Vec<Node>), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("xdelta/0.1.0 (Linux; GTK4)")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Subscription request failed: {}", e))?;

    let mut userinfo: Option<SubscriptionUserinfo> = None;
    if let Some(val) = response.headers().get("subscription-userinfo") {
        if let Ok(info_str) = val.to_str() {
            userinfo = parse_subscription_userinfo(info_str);
        }
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let raw_content = if let Ok(decoded) = BASE64_STANDARD.decode(body.trim()) {
        String::from_utf8(decoded).unwrap_or(body)
    } else if let Ok(decoded) = BASE64_URL_SAFE.decode(body.trim()) {
        String::from_utf8(decoded).unwrap_or(body)
    } else {
        body
    };

    let mut nodes = Vec::new();
    let sub_id = Uuid::new_v4().to_string();

    for line in raw_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Ok(mut node) = parse_node_uri(trimmed) {
            node.subscription_id = Some(sub_id.clone());
            nodes.push(node);
        }
    }

    let sub_name = name.unwrap_or_else(|| {
        url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| "Subscription".into())
    });

    let mut sub = Subscription::new(sub_id, sub_name, url.to_string());
    sub.last_updated = Some(Utc::now());
    sub.userinfo = userinfo;
    sub.node_count = nodes.len();

    Ok((sub, nodes))
}

fn parse_subscription_userinfo(header_str: &str) -> Option<SubscriptionUserinfo> {
    let mut upload = 0u64;
    let mut download = 0u64;
    let mut total = 0u64;
    let mut expire: Option<DateTime<Utc>> = None;

    for part in header_str.split(';') {
        let mut kv = part.trim().split('=');
        if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
            match k.trim() {
                "upload" => upload = v.trim().parse().unwrap_or(0),
                "download" => download = v.trim().parse().unwrap_or(0),
                "total" => total = v.trim().parse().unwrap_or(0),
                "expire" => {
                    if let Ok(ts) = v.trim().parse::<i64>() {
                        expire = DateTime::from_timestamp(ts, 0);
                    }
                }
                _ => {}
            }
        }
    }

    Some(SubscriptionUserinfo {
        upload_bytes: upload,
        download_bytes: download,
        total_bytes: total,
        expire_time: expire,
    })
}
