// ffi.rs — Raw C bindings for titan_audio engine   JCH-2026-011
#![allow(non_camel_case_types, dead_code)]

use std::os::raw::{c_char, c_int, c_float, c_double};

pub type TitanEnginePtr = *mut std::ffi::c_void;

extern "C" {
    pub fn titan_engine_new(alsa_device: *const c_char, bpm: c_double) -> TitanEnginePtr;
    pub fn titan_engine_free(e: TitanEnginePtr);
    pub fn titan_engine_start(e: TitanEnginePtr) -> c_int;
    pub fn titan_engine_stop(e: TitanEnginePtr);

    pub fn titan_engine_play(e: TitanEnginePtr);
    pub fn titan_engine_stop_playback(e: TitanEnginePtr);
    pub fn titan_engine_record_start(e: TitanEnginePtr, track_id: c_int);
    pub fn titan_engine_record_stop(e: TitanEnginePtr);
    pub fn titan_engine_rewind(e: TitanEnginePtr);

    pub fn titan_track_set_volume(e: TitanEnginePtr, id: c_int, vol: c_float);
    pub fn titan_track_set_pan(e: TitanEnginePtr, id: c_int, pan: c_float);
    pub fn titan_track_mute(e: TitanEnginePtr, id: c_int, muted: c_int);
    pub fn titan_track_arm(e: TitanEnginePtr, id: c_int);
    pub fn titan_track_clear(e: TitanEnginePtr, id: c_int);
    pub fn titan_track_export_wav(e: TitanEnginePtr, id: c_int, path: *const c_char) -> c_int;

    pub fn titan_track_peak_l(e: TitanEnginePtr, id: c_int) -> c_float;
    pub fn titan_track_peak_r(e: TitanEnginePtr, id: c_int) -> c_float;
    pub fn titan_master_peak_l(e: TitanEnginePtr) -> c_float;
    pub fn titan_master_peak_r(e: TitanEnginePtr) -> c_float;
    pub fn titan_transport_frame(e: TitanEnginePtr) -> u64;
    pub fn titan_list_devices(out: *mut [c_char; 64], max: c_int) -> c_int;
}
