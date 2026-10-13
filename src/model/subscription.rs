use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionUserinfo {
    pub upload_bytes: u64,
    pub download_bytes: u64,
    pub total_bytes: u64,
    pub expire_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub url: String,
    pub update_interval_minutes: u32,
    pub last_updated: Option<DateTime<Utc>>,
    pub userinfo: Option<SubscriptionUserinfo>,
    pub node_count: usize,
}

impl Subscription {
    pub fn new(id: String, name: String, url: String) -> Self {
        Self {
            id,
            name,
            url,
            update_interval_minutes: 1440, // 24 hours
            last_updated: None,
            userinfo: None,
            node_count: 0,
        }
    }
}
