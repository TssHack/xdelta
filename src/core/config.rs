use crate::model::{Node, Protocol, Security, Transport};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub fn generate_xray_config(node: &Node, socks_port: u16, http_port: u16) -> Result<Value, String> {
    let outbound = build_outbound(node)?;

    let config = json!({
        "log": {
            "loglevel": "warning"
        },
        "inbounds": [
            {
                "tag": "socks-in",
                "port": socks_port,
                "listen": "127.0.0.1",
                "protocol": "socks",
                "settings": {
                    "auth": "noauth",
                    "udp": true
                },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls"]
                }
            },
            {
                "tag": "http-in",
                "port": http_port,
                "listen": "127.0.0.1",
                "protocol": "http",
                "settings": {
                    "allowTransparent": false
                }
            }
        ],
        "outbounds": [
            outbound,
            {
                "tag": "direct",
                "protocol": "freedom",
                "settings": {}
            },
            {
                "tag": "block",
                "protocol": "blackhole",
                "settings": {}
            }
        ],
        "routing": {
            "domainStrategy": "IPIfNonMatch",
            "rules": [
                {
                    "type": "field",
                    "outboundTag": "direct",
                    "ip": ["geoip:private", "geoip:ir"]
                },
                {
                    "type": "field",
                    "outboundTag": "direct",
                    "domain": ["geosite:category-ir"]
                },
                {
                    "type": "field",
                    "outboundTag": "block",
                    "domain": ["geosite:category-ads-all"]
                }
            ]
        }
    });

    Ok(config)
}

fn build_outbound(node: &Node) -> Result<Value, String> {
    match node.protocol {
        Protocol::Vless => {
            let mut user = json!({
                "id": node.secret,
                "encryption": "none"
            });
            if let Some(flow) = &node.flow {
                user["flow"] = json!(flow);
            }

            let mut stream_settings = json!({
                "network": match node.transport {
                    Transport::Ws => "ws",
                    Transport::Grpc => "grpc",
                    _ => "tcp",
                }
            });

            match node.security {
                Security::Reality => {
                    stream_settings["security"] = json!("reality");
                    if let Some(reality) = &node.reality {
                        let mut reality_settings = json!({
                            "serverName": reality.sni,
                            "publicKey": reality.public_key,
                            "fingerprint": "chrome"
                        });
                        if let Some(sid) = &reality.short_id {
                            reality_settings["shortId"] = json!(sid);
                        }
                        if let Some(spx) = &reality.spider_x {
                            reality_settings["spiderX"] = json!(spx);
                        }
                        stream_settings["realitySettings"] = reality_settings;
                    }
                }
                Security::Tls => {
                    stream_settings["security"] = json!("tls");
                    if let Some(sni) = &node.sni {
                        stream_settings["tlsSettings"] = json!({
                            "serverName": sni,
                            "allowInsecure": false
                        });
                    }
                }
                Security::None => {}
            }

            if let Transport::Ws = node.transport {
                if let Some(ws) = &node.ws {
                    let mut ws_settings = json!({ "path": ws.path });
                    if let Some(host) = &ws.host {
                        ws_settings["headers"] = json!({ "Host": host });
                    }
                    stream_settings["wsSettings"] = ws_settings;
                }
            }

            Ok(json!({
                "tag": "proxy",
                "protocol": "vless",
                "settings": {
                    "vnext": [{
                        "address": node.server,
                        "port": node.port,
                        "users": [user]
                    }]
                },
                "streamSettings": stream_settings
            }))
        }
        Protocol::Vmess => {
            let mut stream_settings = json!({
                "network": match node.transport {
                    Transport::Ws => "ws",
                    Transport::Grpc => "grpc",
                    _ => "tcp",
                }
            });

            if let Security::Tls = node.security {
                stream_settings["security"] = json!("tls");
            }

            if let Transport::Ws = node.transport {
                if let Some(ws) = &node.ws {
                    stream_settings["wsSettings"] = json!({ "path": ws.path });
                }
            }

            Ok(json!({
                "tag": "proxy",
                "protocol": "vmess",
                "settings": {
                    "vnext": [{
                        "address": node.server,
                        "port": node.port,
                        "users": [{
                            "id": node.secret,
                            "alterId": 0,
                            "security": "auto"
                        }]
                    }]
                },
                "streamSettings": stream_settings
            }))
        }
        Protocol::Trojan => {
            let mut stream_settings = json!({
                "network": "tcp",
                "security": "tls"
            });
            if let Some(sni) = &node.sni {
                stream_settings["tlsSettings"] = json!({
                    "serverName": sni
                });
            }

            Ok(json!({
                "tag": "proxy",
                "protocol": "trojan",
                "settings": {
                    "servers": [{
                        "address": node.server,
                        "port": node.port,
                        "password": node.secret
                    }]
                },
                "streamSettings": stream_settings
            }))
        }
        _ => Err(format!(
            "Protocol {} is not yet implemented in Xray synthesizer",
            node.protocol.as_str()
        )),
    }
}

pub fn write_config_to_temp_file(config: &Value) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("xdelta_active.json");
    let serialized = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, serialized).map_err(|e| format!("Failed to write config file: {}", e))?;
    Ok(path)
}
