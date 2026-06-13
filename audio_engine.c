/* TitanDAW — ALSA Audio Engine   JCH-2026-011
 * Julius Cameron Hill / TitanU AI LLC
 * Rust calls this via FFI through build.rs / cc crate.
 *
 * Direct ALSA: capture thread + playback thread, lock-free ring buffers.
 * Supports 16 tracks, 48kHz/stereo, 256-frame periods.
 */

#include "audio_engine.h"
#include <alsa/asoundlib.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <math.h>
#include <stdatomic.h>

/* ── Internal state ───────────────────────────────────────────────────────── */

typedef struct {
    TitanEngine        *e;
    snd_pcm_t          *capture_handle;
    snd_pcm_t          *playback_handle;
    pthread_t           capture_thread;
    pthread_t           playback_thread;
    atomic_int          quit;
    float               mix_buf[TITAN_PERIOD_FRAMES * TITAN_CHANNELS];
    float               cap_buf[TITAN_PERIOD_FRAMES * TITAN_CHANNELS];
    int16_t             io_buf[TITAN_PERIOD_FRAMES * TITAN_CHANNELS];
} TitanInternal;

static TitanInternal *g_internal = NULL;

/* ── Helpers ──────────────────────────────────────────────────────────────── */

static float clampf(float v, float lo, float hi) {
    return v < lo ? lo : v > hi ? hi : v;
}

static int alsa_open(snd_pcm_t **h, const char *dev, snd_pcm_stream_t dir) {
    snd_pcm_hw_params_t *params;
    int err;
    unsigned int rate = TITAN_SAMPLE_RATE;
    int periods       = TITAN_PERIODS;
    snd_pcm_uframes_t period = TITAN_PERIOD_FRAMES;

    if ((err = snd_pcm_open(h, dev, dir, 0)) < 0) {
        fprintf(stderr, "[TITAN] ALSA open %s: %s\n", dev, snd_strerror(err));
        return err;
    }
    snd_pcm_hw_params_alloca(&params);
    snd_pcm_hw_params_any(*h, params);
    snd_pcm_hw_params_set_access(*h, params, SND_PCM_ACCESS_RW_INTERLEAVED);
    snd_pcm_hw_params_set_format(*h, params, SND_PCM_FORMAT_S16_LE);
    snd_pcm_hw_params_set_channels(*h, params, TITAN_CHANNELS);
    snd_pcm_hw_params_set_rate_near(*h, params, &rate, 0);
    snd_pcm_hw_params_set_period_size_near(*h, params, &period, 0);
    snd_pcm_hw_params_set_periods(*h, params, periods, 0);
    if ((err = snd_pcm_hw_params(*h, params)) < 0) {
        fprintf(stderr, "[TITAN] hw_params %s: %s\n", dev, snd_strerror(err));
        return err;
    }
    snd_pcm_prepare(*h);
    return 0;
}

/* ── Capture thread ───────────────────────────────────────────────────────── */

static void *capture_thread(void *arg) {
    TitanInternal *in = (TitanInternal *)arg;
    TitanEngine   *e  = in->e;

    while (!atomic_load(&in->quit)) {
        int frames = snd_pcm_readi(in->capture_handle, in->io_buf, TITAN_PERIOD_FRAMES);
        if (frames < 0) {
            snd_pcm_recover(in->capture_handle, frames, 1);
            continue;
        }

        if (!e->recording) continue;

        int tid = e->armed_track;
        if (tid < 0 || tid >= TITAN_MAX_TRACKS) continue;
        TitanTrack *t = &e->tracks[tid];
        if (t->state != TRACK_RECORDING) continue;

        float peak_l = 0, peak_r = 0;
        for (int i = 0; i < frames; i++) {
            float l = in->io_buf[i*2]   / 32768.0f;
            float r = in->io_buf[i*2+1] / 32768.0f;

            size_t wi = (t->ring_write + i * 2)     % TITAN_RING_FRAMES;
            size_t wi2= (t->ring_write + i * 2 + 1) % TITAN_RING_FRAMES;
            t->ring[wi]  = l;
            t->ring[wi2] = r;

            if (fabsf(l) > peak_l) peak_l = fabsf(l);
            if (fabsf(r) > peak_r) peak_r = fabsf(r);
        }
        t->ring_write      = (t->ring_write + frames * 2) % TITAN_RING_FRAMES;
        t->recorded_frames += frames;
        t->peak_l           = peak_l;
        t->peak_r           = peak_r;
        e->transport_frame += frames;
    }
    return NULL;
}

/* ── Playback thread ──────────────────────────────────────────────────────── */

static void *playback_thread(void *arg) {
    TitanInternal *in = (TitanInternal *)arg;
    TitanEngine   *e  = in->e;

    while (!atomic_load(&in->quit)) {
        if (!e->playing) {
            /* silence while stopped */
            memset(in->io_buf, 0, sizeof(in->io_buf));
            snd_pcm_writei(in->playback_handle, in->io_buf, TITAN_PERIOD_FRAMES);
            continue;
        }

        /* mix all playing tracks */
        memset(in->mix_buf, 0, sizeof(in->mix_buf));
        float master_l = 0, master_r = 0;

        for (int tid = 0; tid < TITAN_MAX_TRACKS; tid++) {
            TitanTrack *t = &e->tracks[tid];
            if (t->state != TRACK_PLAYING || t->recorded_frames == 0) continue;

            float pan_l = (t->pan <= 0) ? 1.0f : 1.0f - t->pan;
            float pan_r = (t->pan >= 0) ? 1.0f : 1.0f + t->pan;

            for (int i = 0; i < TITAN_PERIOD_FRAMES; i++) {
                size_t ri  = (t->ring_read + i * 2)     % TITAN_RING_FRAMES;
                size_t ri2 = (t->ring_read + i * 2 + 1) % TITAN_RING_FRAMES;
                float l = t->ring[ri]  * t->volume * pan_l;
                float r = t->ring[ri2] * t->volume * pan_r;
                in->mix_buf[i*2]   += l;
                in->mix_buf[i*2+1] += r;
                if (fabsf(l) > master_l) master_l = fabsf(l);
                if (fabsf(r) > master_r) master_r = fabsf(r);
            }
            t->ring_read = (t->ring_read + TITAN_PERIOD_FRAMES * 2) % TITAN_RING_FRAMES;
        }

        e->master_peak_l = master_l * e->master_volume;
        e->master_peak_r = master_r * e->master_volume;

        /* convert to int16 */
        for (int i = 0; i < TITAN_PERIOD_FRAMES * TITAN_CHANNELS; i++) {
            float s = clampf(in->mix_buf[i] * e->master_volume, -1.0f, 1.0f);
            in->io_buf[i] = (int16_t)(s * 32767.0f);
        }

        int err = snd_pcm_writei(in->playback_handle, in->io_buf, TITAN_PERIOD_FRAMES);
        if (err < 0) snd_pcm_recover(in->playback_handle, err, 1);
    }
    return NULL;
}

/* ── Public API ───────────────────────────────────────────────────────────── */

TitanEngine *titan_engine_new(const char *alsa_device, double bpm) {
    TitanEngine *e = (TitanEngine *)calloc(1, sizeof(TitanEngine));
    strncpy(e->device, alsa_device, 63);
    e->bpm           = bpm;
    e->master_volume = 0.85f;
    e->armed_track   = 0;

    for (int i = 0; i < TITAN_MAX_TRACKS; i++) {
        e->tracks[i].id     = i;
        e->tracks[i].volume = 0.8f;
        e->tracks[i].pan    = 0.0f;
        e->tracks[i].state  = TRACK_EMPTY;
        e->tracks[i].ring   = (float *)calloc(TITAN_RING_FRAMES, sizeof(float));
        snprintf(e->tracks[i].name, 32, "Track %d", i+1);
    }
    return e;
}

void titan_engine_free(TitanEngine *e) {
    if (!e) return;
    titan_engine_stop(e);
    for (int i = 0; i < TITAN_MAX_TRACKS; i++)
        free(e->tracks[i].ring);
    free(e);
}

int titan_engine_start(TitanEngine *e) {
    TitanInternal *in = (TitanInternal *)calloc(1, sizeof(TitanInternal));
    in->e = e;
    atomic_store(&in->quit, 0);
    g_internal = in;

    if (alsa_open(&in->capture_handle,  e->device, SND_PCM_STREAM_CAPTURE)  < 0) return -1;
    if (alsa_open(&in->playback_handle, e->device, SND_PCM_STREAM_PLAYBACK) < 0) return -2;

    e->running = 1;
    pthread_create(&in->capture_thread,  NULL, capture_thread,  in);
    pthread_create(&in->playback_thread, NULL, playback_thread, in);
    return 0;
}

void titan_engine_stop(TitanEngine *e) {
    if (!g_internal || !e->running) return;
    atomic_store(&g_internal->quit, 1);
    pthread_join(g_internal->capture_thread,  NULL);
    pthread_join(g_internal->playback_thread, NULL);
    snd_pcm_close(g_internal->capture_handle);
    snd_pcm_close(g_internal->playback_handle);
    free(g_internal);
    g_internal = NULL;
    e->running = 0;
}

void titan_engine_play(TitanEngine *e) {
    for (int i = 0; i < TITAN_MAX_TRACKS; i++)
        if (e->tracks[i].state == TRACK_ARMED || e->tracks[i].recorded_frames > 0)
            if (e->tracks[i].state != TRACK_MUTED)
                e->tracks[i].state = TRACK_PLAYING;
    e->playing = 1;
}

void titan_engine_stop_playback(TitanEngine *e) {
    e->playing = 0;
}

void titan_engine_record_start(TitanEngine *e, int track_id) {
    if (track_id < 0 || track_id >= TITAN_MAX_TRACKS) return;
    e->armed_track = track_id;
    e->tracks[track_id].state  = TRACK_RECORDING;
    e->tracks[track_id].ring_write = 0;
    e->tracks[track_id].ring_read  = 0;
    e->tracks[track_id].recorded_frames = 0;
    e->recording = 1;
    e->playing   = 1;
}

void titan_engine_record_stop(TitanEngine *e) {
    int tid = e->armed_track;
    e->recording = 0;
    if (tid >= 0 && tid < TITAN_MAX_TRACKS)
        e->tracks[tid].state = TRACK_PLAYING;
}

void titan_engine_rewind(TitanEngine *e) {
    e->transport_frame = 0;
    for (int i = 0; i < TITAN_MAX_TRACKS; i++)
        e->tracks[i].ring_read = 0;
}

void titan_track_set_volume(TitanEngine *e, int id, float vol) {
    if (id < 0 || id >= TITAN_MAX_TRACKS) return;
    e->tracks[id].volume = clampf(vol, 0.0f, 1.0f);
}

void titan_track_set_pan(TitanEngine *e, int id, float pan) {
    if (id < 0 || id >= TITAN_MAX_TRACKS) return;
    e->tracks[id].pan = clampf(pan, -1.0f, 1.0f);
}

void titan_track_mute(TitanEngine *e, int id, int muted) {
    if (id < 0 || id >= TITAN_MAX_TRACKS) return;
    e->tracks[id].state = muted ? TRACK_MUTED : TRACK_PLAYING;
}

void titan_track_arm(TitanEngine *e, int id) {
    if (id < 0 || id >= TITAN_MAX_TRACKS) return;
    e->armed_track    = id;
    e->tracks[id].state = TRACK_ARMED;
}

void titan_track_clear(TitanEngine *e, int id) {
    if (id < 0 || id >= TITAN_MAX_TRACKS) return;
    memset(e->tracks[id].ring, 0, TITAN_RING_FRAMES * sizeof(float));
    e->tracks[id].recorded_frames = 0;
    e->tracks[id].ring_write = 0;
    e->tracks[id].ring_read  = 0;
    e->tracks[id].state      = TRACK_EMPTY;
}

int titan_track_export_wav(TitanEngine *e, int id, const char *path) {
    if (id < 0 || id >= TITAN_MAX_TRACKS) return -1;
    TitanTrack *t = &e->tracks[id];
    if (t->recorded_frames == 0) return -2;

    FILE *f = fopen(path, "wb");
    if (!f) return -3;

    uint32_t data_size   = t->recorded_frames * TITAN_CHANNELS * 2;
    uint32_t chunk_size  = 36 + data_size;
    uint32_t sample_rate = TITAN_SAMPLE_RATE;
    uint32_t byte_rate   = TITAN_SAMPLE_RATE * TITAN_CHANNELS * 2;
    uint16_t block_align = TITAN_CHANNELS * 2;
    uint16_t bits        = 16;
    uint16_t num_ch      = TITAN_CHANNELS;
    uint16_t audio_fmt   = 1;

    /* WAV header */
    fwrite("RIFF", 1, 4, f); fwrite(&chunk_size, 4, 1, f);
    fwrite("WAVE", 1, 4, f);
    fwrite("fmt ", 1, 4, f);
    uint32_t fmt_size = 16; fwrite(&fmt_size, 4, 1, f);
    fwrite(&audio_fmt,   2, 1, f); fwrite(&num_ch,      2, 1, f);
    fwrite(&sample_rate, 4, 1, f); fwrite(&byte_rate,   4, 1, f);
    fwrite(&block_align, 2, 1, f); fwrite(&bits,        2, 1, f);
    fwrite("data", 1, 4, f); fwrite(&data_size, 4, 1, f);

    for (size_t i = 0; i < t->recorded_frames * TITAN_CHANNELS; i++) {
        float s = clampf(t->ring[i], -1.0f, 1.0f);
        int16_t s16 = (int16_t)(s * 32767.0f);
        fwrite(&s16, 2, 1, f);
    }
    fclose(f);
    return 0;
}

float titan_track_peak_l(TitanEngine *e, int id)  { return (id>=0&&id<TITAN_MAX_TRACKS)?e->tracks[id].peak_l:0; }
float titan_track_peak_r(TitanEngine *e, int id)  { return (id>=0&&id<TITAN_MAX_TRACKS)?e->tracks[id].peak_r:0; }
float titan_master_peak_l(TitanEngine *e)          { return e->master_peak_l; }
float titan_master_peak_r(TitanEngine *e)          { return e->master_peak_r; }
uint64_t titan_transport_frame(TitanEngine *e)     { return e->transport_frame; }

int titan_list_devices(char out[][64], int max) {
    void **hints;
    
    if (snd_device_name_hint(-1, "pcm", &hints) < 0) {
        return 0;
    }
    
    int count = 0;
    void **p = hints;
    
    while (*p && count < max) {
        char *name = snd_device_name_get_hint(*p, "NAME");
        if (name) {
            strncpy(out[count], name, 63);
            out[count][63] = 0;
            free(name);
            count++;
        }
        p++;
    }
    
    snd_device_name_free_hint(hints);
    return count;
}
