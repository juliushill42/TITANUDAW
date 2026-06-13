#pragma once
#include <stdint.h>
#include <stddef.h>

/* ── TitanDAW ALSA Audio Engine ── JCH-2026-011 ─────────────────────────── */

#define TITAN_MAX_TRACKS     16
#define TITAN_SAMPLE_RATE    48000
#define TITAN_CHANNELS       2
#define TITAN_PERIOD_FRAMES  256
#define TITAN_PERIODS        4
#define TITAN_RING_FRAMES    (TITAN_SAMPLE_RATE * 30)   /* 30s ring per track */

typedef enum {
    TRACK_EMPTY   = 0,
    TRACK_ARMED   = 1,
    TRACK_RECORDING = 2,
    TRACK_PLAYING = 3,
    TRACK_MUTED   = 4,
} TrackState;

typedef struct {
    int          id;
    TrackState   state;
    float        volume;          /* 0.0 – 1.0 */
    float        pan;             /* -1.0 left, 0 center, 1.0 right */
    float       *ring;            /* circular PCM buffer, stereo interleaved */
    size_t       ring_write;      /* write head */
    size_t       ring_read;       /* playback head */
    size_t       recorded_frames; /* how many frames captured */
    float        peak_l;          /* live VU peak L */
    float        peak_r;          /* live VU peak R */
    char         name[32];
} TitanTrack;

typedef struct {
    int          running;
    int          recording;       /* global record arm */
    int          playing;
    double       bpm;
    int          armed_track;     /* which track is armed for input */
    TitanTrack   tracks[TITAN_MAX_TRACKS];
    float        master_volume;
    float        master_peak_l;
    float        master_peak_r;
    uint64_t     transport_frame; /* current playback position */
    char         device[64];      /* ALSA device, e.g. "hw:1,0" */
} TitanEngine;

/* Lifecycle */
TitanEngine *titan_engine_new(const char *alsa_device, double bpm);
void         titan_engine_free(TitanEngine *e);
int          titan_engine_start(TitanEngine *e);   /* spawns capture+playback threads */
void         titan_engine_stop(TitanEngine *e);

/* Transport */
void titan_engine_play(TitanEngine *e);
void titan_engine_stop_playback(TitanEngine *e);
void titan_engine_record_start(TitanEngine *e, int track_id);
void titan_engine_record_stop(TitanEngine *e);
void titan_engine_rewind(TitanEngine *e);

/* Track ops */
void titan_track_set_volume(TitanEngine *e, int track_id, float vol);
void titan_track_set_pan(TitanEngine *e, int track_id, float pan);
void titan_track_mute(TitanEngine *e, int track_id, int muted);
void titan_track_arm(TitanEngine *e, int track_id);
void titan_track_clear(TitanEngine *e, int track_id);
int  titan_track_export_wav(TitanEngine *e, int track_id, const char *path);

/* Metering — called from UI thread, thread-safe reads */
float titan_track_peak_l(TitanEngine *e, int track_id);
float titan_track_peak_r(TitanEngine *e, int track_id);
float titan_master_peak_l(TitanEngine *e);
float titan_master_peak_r(TitanEngine *e);
uint64_t titan_transport_frame(TitanEngine *e);

/* ALSA device discovery */
int  titan_list_devices(char out[][64], int max);
