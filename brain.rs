// brain.rs — SmolLM inference client + AI alert engine   JCH-2026-011
// Calls titan_producer_brain.py (port 5002) async, sends BrainAlerts to UI.
use serde::{Deserialize, Serialize};
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::thread;
use log::{info, warn};

#[derive(Debug, Clone)]
pub enum AlertLevel {
    Info,
    Warn,
    Critical,
}

#[derive(Debug, Clone)]
pub struct BrainAlert {
    pub level:   AlertLevel,
    pub message: String,
    pub tag:     String,   // "pitch", "timing", "mix", "lyric", "beat"
}

#[derive(Serialize)]
struct ProduceReq<'a> {
    description: &'a str,
    execute: bool,
}

#[derive(Deserialize)]
struct BrainResp {
    ok: bool,
    #[serde(default)]
    response: String,
    #[serde(default)]
    raw: String,
}

pub struct Brain {
    pub alert_rx: Receiver<BrainAlert>,
    cmd_tx:       Sender<String>,
}

impl Brain {
    pub fn new(brain_url: &str) -> Self {
        let (alert_tx, alert_rx): (Sender<BrainAlert>, Receiver<BrainAlert>) = unbounded();
        let (cmd_tx,   cmd_rx):   (Sender<String>,     Receiver<String>)     = unbounded();

        let url = brain_url.to_string();
        thread::spawn(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap();

            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => {
                        let endpoint = if cmd.starts_with("chat:") {
                            format!("{}/brain/chat", url)
                        } else {
                            format!("{}/brain/produce", url)
                        };

                        let body = if cmd.starts_with("chat:") {
                            serde_json::json!({ "message": &cmd[5..] })
                        } else {
                            serde_json::json!({ "description": cmd, "execute": true })
                        };

                        match client.post(&endpoint).json(&body).send() {
                            Ok(resp) => {
                                if let Ok(data) = resp.json::<serde_json::Value>() {
                                    let text = data["response"]
                                        .as_str()
                                        .or_else(|| data["raw"].as_str())
                                        .unwrap_or("").to_string();

                                    // Parse AI response for alerts
                                    let alerts = parse_alerts(&text);
                                    for a in alerts {
                                        let _ = alert_tx.send(a);
                                    }

                                    info!("Brain: {}", &text[..text.len().min(120)]);
                                }
                            }
                            Err(e) => {
                                warn!("Brain unreachable: {e}");
                                let _ = alert_tx.send(BrainAlert {
                                    level:   AlertLevel::Warn,
                                    message: format!("Brain offline: {e}"),
                                    tag:     "system".into(),
                                });
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self { alert_rx, cmd_tx }
    }

    pub fn ask(&self, prompt: &str) {
        let _ = self.cmd_tx.send(prompt.to_string());
    }

    pub fn chat(&self, msg: &str) {
        let _ = self.cmd_tx.send(format!("chat:{}", msg));
    }

    /// Called periodically from UI: drain alerts
    pub fn drain_alerts(&self) -> Vec<BrainAlert> {
        let mut out = Vec::new();
        while let Ok(a) = self.alert_rx.try_recv() {
            out.push(a);
        }
        out
    }
}

/// Heuristic: scan AI text for keywords → typed alerts
fn parse_alerts(text: &str) -> Vec<BrainAlert> {
    let mut alerts = Vec::new();
    let lower = text.to_lowercase();

    let checks: &[(&str, &str, AlertLevel)] = &[
        ("off-key",      "pitch",   AlertLevel::Warn),
        ("out of tune",  "pitch",   AlertLevel::Warn),
        ("pitch issue",  "pitch",   AlertLevel::Warn),
        ("clipping",     "mix",     AlertLevel::Critical),
        ("too loud",     "mix",     AlertLevel::Warn),
        ("timing off",   "timing",  AlertLevel::Warn),
        ("out of time",  "timing",  AlertLevel::Warn),
        ("lyric",        "lyric",   AlertLevel::Info),
        ("melody",       "beat",    AlertLevel::Info),
        ("808",          "beat",    AlertLevel::Info),
    ];

    for (kw, tag, level) in checks {
        if lower.contains(kw) {
            alerts.push(BrainAlert {
                level:   level.clone(),
                message: text.lines()
                    .find(|l| l.to_lowercase().contains(kw))
                    .unwrap_or(kw)
                    .trim()
                    .to_string(),
                tag:     tag.to_string(),
            });
        }
    }

    if alerts.is_empty() && !text.is_empty() {
        alerts.push(BrainAlert {
            level:   AlertLevel::Info,
            message: text.lines().next().unwrap_or("").trim().to_string(),
            tag:     "ai".into(),
        });
    }

    alerts
}
