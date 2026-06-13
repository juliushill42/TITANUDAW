// session.rs   JCH-2026-011
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::Result;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SessionState {
    pub version:        u32,
    pub bpm:            f64,
    pub volumes:        Vec<f32>,
    pub pans:           Vec<f32>,
    pub pitches:        Vec<f32>,
    pub muted:          Vec<bool>,
    pub soloed:         Vec<bool>,
    pub track_names:    Vec<String>,
    pub loop_start_sec: f64,
    pub loop_end_sec:   f64,
    pub loop_active:    bool,
    pub metronome:      bool,
    pub alsa_device:    String,
    pub buffer_frames:  i32,
    pub wav_paths:      Vec<String>,
}

pub fn default_session_path() -> PathBuf {
    let mut p = dirs::document_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("TitanDAW");
    std::fs::create_dir_all(&p).ok();
    p.push("session.json");
    p
}

pub fn save(state: &SessionState, path: &PathBuf) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

pub fn load(path: &PathBuf) -> Result<SessionState> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}
