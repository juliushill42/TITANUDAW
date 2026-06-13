// engine.rs — Thread-safe Rust wrapper over C audio engine   JCH-2026-011
use std::ffi::CString;
use std::sync::{Arc, Mutex};
use crate::ffi::*;

pub struct AudioEngine {
    ptr:      TitanEnginePtr,
    pitches:  [f32; 16],
    soloed:   [bool; 16],
    names:    [String; 16],
    metronome: bool,
    bpm:      f64,
    loop_start: i64,
    loop_end:   i64,
    loop_active: bool,
    wav_paths:  [String; 16],
}

unsafe impl Send for AudioEngine {}
unsafe impl Sync for AudioEngine {}

impl AudioEngine {
    pub fn new(device: &str, bpm: f64, _buffer_frames: i32) -> Arc<Mutex<Self>> {
        let dev = CString::new(device).unwrap();
        let ptr = unsafe { titan_engine_new(dev.as_ptr(), bpm) };
        Arc::new(Mutex::new(Self {
            ptr,
            pitches:  [1.0; 16],
            soloed:   [false; 16],
            names:    std::array::from_fn(|i| format!("Track {:02}", i+1)),
            metronome: false,
            bpm,
            loop_start: 0,
            loop_end:   0,
            loop_active: false,
            wav_paths:  std::array::from_fn(|_| String::new()),
        }))
    }

    pub fn play(&mut self) { unsafe { titan_engine_play(self.ptr) } }
    pub fn stop_playback(&mut self) { unsafe { titan_engine_stop_playback(self.ptr) } }
    pub fn record_start(&mut self, id: i32) { unsafe { titan_engine_record_start(self.ptr, id) } }
    pub fn record_stop(&mut self, _id: i32) { unsafe { titan_engine_record_stop(self.ptr) } }
    pub fn rewind(&mut self) { unsafe { titan_engine_rewind(self.ptr) } }
    pub fn arm(&mut self, id: i32) { unsafe { titan_track_arm(self.ptr, id) } }

    pub fn set_volume(&mut self, id: i32, v: f32) {
        unsafe { titan_track_set_volume(self.ptr, id, v) }
    }
    pub fn set_pan(&mut self, id: i32, p: f32) {
        unsafe { titan_track_set_pan(self.ptr, id, p) }
    }
    pub fn mute(&mut self, id: i32, m: bool) {
        unsafe { titan_track_mute(self.ptr, id, m as i32) }
    }
    pub fn set_pitch(&mut self, id: i32, p: f32) {
        if id >= 0 && id < 16 { self.pitches[id as usize] = p; }
    }
    pub fn solo(&mut self, id: i32, s: bool) {
        if id >= 0 && id < 16 { self.soloed[id as usize] = s; }
    }
    pub fn set_track_name(&mut self, id: i32, name: &str) {
        if id >= 0 && id < 16 { self.names[id as usize] = name.to_string(); }
    }
    pub fn set_metronome(&mut self, m: bool) { self.metronome = m; }
    pub fn set_bpm(&mut self, bpm: f64) { self.bpm = bpm; }
    pub fn set_loop(&mut self, start: i64, end: i64, active: bool) {
        self.loop_start  = start;
        self.loop_end    = end;
        self.loop_active = active;
    }
    pub fn load_wav(&mut self, id: i32, path: &str) -> i64 {
        if id >= 0 && id < 16 {
            self.wav_paths[id as usize] = path.to_string();
        }
        // stub: return 1 frame to signal success; real impl reads WAV
        1
    }
    pub fn export_wav(&mut self, id: i32, path: &str) -> i32 {
        let p = CString::new(path).unwrap();
        unsafe { titan_track_export_wav(self.ptr, id, p.as_ptr()) }
    }

    pub fn peak_l(&self, id: i32) -> f32 { unsafe { titan_track_peak_l(self.ptr, id) } }
    pub fn peak_r(&self, id: i32) -> f32 { unsafe { titan_track_peak_r(self.ptr, id) } }
    pub fn master_peak_l(&self) -> f32   { unsafe { titan_master_peak_l(self.ptr) } }
    pub fn master_peak_r(&self) -> f32   { unsafe { titan_master_peak_r(self.ptr) } }
    pub fn transport_frame(&self) -> u64 { unsafe { titan_transport_frame(self.ptr) } }

    pub fn list_devices() -> Vec<String> {
        let mut buf = [[0i8; 64]; 32];
        let count = unsafe { titan_list_devices(buf.as_mut_ptr(), 32) };
        (0..count as usize).map(|i| {
            let bytes: Vec<u8> = buf[i].iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8).collect();
            String::from_utf8_lossy(&bytes).into_owned()
        }).collect()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) { unsafe { titan_engine_free(self.ptr) } }
}

pub type SharedEngine = Arc<Mutex<AudioEngine>>;
