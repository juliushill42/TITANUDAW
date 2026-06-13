// ui.rs — GTK4 + Cairo custom DAW UI   JCH-2026-011
// Every pixel drawn with Cairo — no stock GTK widgets for the DAW canvas.

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Box as GBox, Orientation, DrawingArea,
          Button, Label, Scale, Entry, ScrolledWindow, TextView, TextBuffer,
          Align, Separator};
use cairo::Context;
use glib::timeout_add_local;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::engine::{AudioEngine, AudioEngineReader};
use crate::brain::{Brain, AlertLevel};
use crate::midi::MidiRouter;

// ── Palette ──────────────────────────────────────────────────────────────────

const SURFACE:  (f64,f64,f64) = (0.086, 0.106, 0.133);   // #161b22
const BORDER:   (f64,f64,f64) = (0.188, 0.212, 0.239);   // #30363d
const ACCENT:   (f64,f64,f64) = (0.0,   1.0,   0.616);   // #00ff9d
const ORANGE:   (f64,f64,f64) = (1.0,   0.420, 0.208);   // #ff6b35
const RED:      (f64,f64,f64) = (0.973, 0.318, 0.286);   // #f85149
const PURPLE:   (f64,f64,f64) = (0.737, 0.549, 1.0);     // #bc8cff
const TEXT:     (f64,f64,f64) = (0.902, 0.929, 0.953);   // #e6edf3
const MUTED:    (f64,f64,f64) = (0.490, 0.522, 0.565);   // #7d8590
const YELLOW:   (f64,f64,f64) = (0.890, 0.702, 0.255);   // #e3b341

const TRACK_H:  f64 = 64.0;
const TRACK_W:  f64 = 900.0;
const RULER_H:  f64 = 24.0;
const MAX_TRACKS: usize = 16;

fn set_color(cr: &Context, (r,g,b): (f64,f64,f64)) {
    cr.set_source_rgb(r, g, b);
}
fn set_color_a(cr: &Context, (r,g,b): (f64,f64,f64), a: f64) {
    cr.set_source_rgba(r, g, b, a);
}

// ── Shared UI State ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct UiState {
    selected_track: usize,
    muted:          [bool; MAX_TRACKS],
    volumes:        [f32; MAX_TRACKS],
    pans:           [f32; MAX_TRACKS],
    bpm:            f64,
    is_playing:     bool,
    is_recording:   bool,
    transport_sec:  f64,
    peaks_l:        [f32; MAX_TRACKS],
    peaks_r:        [f32; MAX_TRACKS],
    master_l:       f32,
    master_r:       f32,
    alerts:         Vec<(AlertLevel, String, String)>,
    alsa_device:    String,
}

impl Default for UiState {
    fn default() -> Self {
        let volumes = [0.8f32; MAX_TRACKS];
        Self {
            selected_track: 0,
            muted:          [false; MAX_TRACKS],
            volumes,
            pans:           [0.0f32; MAX_TRACKS],
            bpm:            140.0,
            is_playing:     false,
            is_recording:   false,
            transport_sec:  0.0,
            peaks_l:        [0.0; MAX_TRACKS],
            peaks_r:        [0.0; MAX_TRACKS],
            master_l:       0.0,
            master_r:       0.0,
            alerts:         Vec::new(),
            alsa_device:    "hw:1,0".into(),
        }
    }
}

// ── Transport Bar (Cairo) ─────────────────────────────────────────────────────

fn draw_transport(cr: &Context, w: f64, state: &UiState) {
    set_color(cr, SURFACE);
    cr.rectangle(0.0, 0.0, w, 48.0);
    cr.fill().ok();

    set_color(cr, BORDER);
    cr.set_line_width(1.0);
    cr.move_to(0.0, 47.5);
    cr.line_to(w, 47.5);
    cr.stroke().ok();

    set_color(cr, ACCENT);
    cr.set_font_size(20.0);
    cr.move_to(16.0, 32.0);
    cr.show_text(&format!("{:.1} BPM", state.bpm)).ok();

    let secs = state.transport_sec as u64;
    let ms   = ((state.transport_sec - secs as f64) * 1000.0) as u64;
    set_color(cr, TEXT);
    cr.set_font_size(14.0);
    cr.move_to(160.0, 30.0);
    cr.show_text(&format!("{:02}:{:02}.{:03}", secs/60, secs%60, ms)).ok();

    if state.is_recording {
        set_color(cr, RED);
        cr.arc(w/2.0, 24.0, 8.0, 0.0, std::f64::consts::TAU);
        cr.fill().ok();
        set_color(cr, TEXT);
        cr.set_font_size(11.0);
        cr.move_to(w/2.0 + 14.0, 28.0);
        cr.show_text("REC").ok();
    } else if state.is_playing {
        set_color(cr, ACCENT);
        cr.set_font_size(11.0);
        cr.move_to(w/2.0 + 14.0, 28.0);
        cr.show_text("PLAYING").ok();
    }

    let vu_x = w - 140.0;
    draw_vu_pair(cr, vu_x, 8.0, 120.0, 14.0, state.master_l, state.master_r);
    set_color(cr, MUTED);
    cr.set_font_size(9.0);
    cr.move_to(vu_x, 38.0);
    cr.show_text("MASTER").ok();
}

fn draw_vu_pair(cr: &Context, x: f64, y: f64, w: f64, h: f64, l: f32, r: f32) {
    let half = h / 2.0 - 1.0;
    for (i, peak) in [l, r].iter().enumerate() {
        let bar_y = y + i as f64 * (half + 2.0);
        set_color_a(cr, BORDER, 0.6);
        cr.rectangle(x, bar_y, w, half);
        cr.fill().ok();
        
        let fill_w = (w * (*peak as f64)).min(w);
        let color = if *peak > 0.9 { RED } else if *peak > 0.7 { YELLOW } else { ACCENT };
        set_color(cr, color);
        cr.rectangle(x, bar_y, fill_w, half);
        cr.fill().ok();
    }
}

// ── Track Lane (Cairo) ────────────────────────────────────────────────────────

fn draw_tracks(cr: &Context, state: &UiState) {
    let header_w = 180.0;

    set_color(cr, SURFACE);
    cr.rectangle(header_w, 0.0, TRACK_W, RULER_H);
    cr.fill().ok();

    set_color(cr, MUTED);
    cr.set_font_size(9.0);
    cr.set_line_width(0.5);
    for beat in 0..64 {
        let rx = header_w + beat as f64 * 80.0;
        cr.move_to(rx, 0.0);
        cr.line_to(rx, if beat % 4 == 0 { RULER_H } else { RULER_H * 0.5 });
        cr.stroke().ok();
        if beat % 4 == 0 {
            cr.move_to(rx + 2.0, RULER_H - 4.0);
            cr.show_text(&format!("{}", beat/4 + 1)).ok();
        }
    }

    let cursor_x = header_w + state.transport_sec * 80.0 * (state.bpm / 60.0);
    set_color(cr, ORANGE);
    cr.set_line_width(1.5);
    cr.move_to(cursor_x, 0.0);
    cr.line_to(cursor_x, RULER_H + MAX_TRACKS as f64 * TRACK_H);
    cr.stroke().ok();

    for i in 0..MAX_TRACKS {
        let ty = RULER_H + i as f64 * TRACK_H;

        let hdr_color = if i == state.selected_track { (0.12, 0.18, 0.24) } else { SURFACE };
        set_color(cr, hdr_color);
        cr.rectangle(0.0, ty, header_w, TRACK_H);
        cr.fill().ok();

        set_color(cr, if i == state.selected_track { ACCENT } else { TEXT });
        cr.set_font_size(11.0);
        cr.move_to(8.0, ty + 18.0);
        cr.show_text(&format!("T{:02}", i+1)).ok();

        draw_vu_pair(cr, 36.0, ty + 8.0, 60.0, 20.0, state.peaks_l[i], state.peaks_r[i]);

        if state.muted[i] {
            set_color(cr, YELLOW);
            cr.set_font_size(8.0);
            cr.move_to(100.0, ty + 18.0);
            cr.show_text("MUTE").ok();
        }

        let pan_x = 140.0 + state.pans[i] as f64 * 18.0;
        set_color(cr, PURPLE);
        cr.arc(pan_x, ty + 14.0, 3.0, 0.0, std::f64::consts::TAU);
        cr.fill().ok();

        let vol_h   = TRACK_H - 12.0;
        let filled  = vol_h * state.volumes[i] as f64;
        set_color_a(cr, BORDER, 0.8);
        cr.rectangle(header_w - 10.0, ty + 6.0, 6.0, vol_h);
        cr.fill().ok();
        set_color(cr, ACCENT);
        cr.rectangle(header_w - 10.0, ty + 6.0 + (vol_h - filled), 6.0, filled);
        cr.fill().ok();

        let region_color = if i == state.selected_track { (0.086, 0.118, 0.145) } else { (0.063, 0.082, 0.102) };
        set_color(cr, region_color);
        cr.rectangle(header_w, ty, TRACK_W, TRACK_H - 1.0);
        cr.fill().ok();

        if state.peaks_l[i] > 0.001 || state.peaks_r[i] > 0.001 {
            set_color_a(cr, ACCENT, 0.25);
            cr.rectangle(header_w + 2.0, ty + 4.0, 200.0, TRACK_H - 8.0);
            cr.fill().ok();

            set_color_a(cr, ACCENT, 0.5);
            cr.set_line_width(1.0);
            let mid = ty + TRACK_H / 2.0;
            for x in (0..200).step_by(4) {
                let amp = (state.peaks_l[i] as f64 * 20.0 * (x as f64 * 0.3).sin().abs()) + 1.0;
                cr.move_to(header_w + 2.0 + x as f64, mid - amp);
                cr.line_to(header_w + 2.0 + x as f64, mid + amp);
                cr.stroke().ok();
            }
        }

        set_color(cr, BORDER);
        cr.set_line_width(0.5);
        cr.move_to(0.0, ty + TRACK_H - 0.5);
        cr.line_to(header_w + TRACK_W, ty + TRACK_H - 0.5);
        cr.stroke().ok();
    }
}

// ── AI Alert Strip (Cairo) ────────────────────────────────────────────────────

fn draw_alert_strip(cr: &Context, w: f64, alerts: &[(AlertLevel, String, String)]) {
    set_color(cr, SURFACE);
    cr.rectangle(0.0, 0.0, w, 32.0);
    cr.fill().ok();

    if let Some((level, msg, tag)) = alerts.last() {
        let color = match level {
            AlertLevel::Critical => RED,
            AlertLevel::Warn     => YELLOW,
            AlertLevel::Info     => ACCENT,
        };
        set_color(cr, color);
        cr.rectangle(0.0, 0.0, 4.0, 32.0);
        cr.fill().ok();

        set_color_a(cr, color, 0.2);
        cr.rectangle(8.0, 6.0, 50.0, 20.0);
        cr.fill().ok();
        set_color(cr, color);
        cr.set_font_size(9.0);
        cr.move_to(12.0, 20.0);
        cr.show_text(&tag.to_uppercase()).ok();

        set_color(cr, TEXT);
        cr.set_font_size(11.0);
        cr.move_to(68.0, 21.0);
        let short = if msg.len() > 120 { &msg[..120] } else { msg.as_str() };
        cr.show_text(short).ok();
    } else {
        set_color(cr, MUTED);
        cr.set_font_size(10.0);
        cr.move_to(8.0, 21.0);
        cr.show_text("AI BRAIN — STANDBY").ok();
    }
}

// ── Build & Show ──────────────────────────────────────────────────────────────

pub fn build_and_show(app: &Application) {
    let state = Arc::new(Mutex::new(UiState::default()));

    let devices = AudioEngine::list_devices();
    {
        let mut s = state.lock().unwrap();
        if let Some(dev) = devices.iter().find(|d| {
            let l = d.to_lowercase();
            l.contains("usb") || l.contains("audiobox") || l.contains("hw:1")
        }) {
            s.alsa_device = dev.clone();
        }
    }

    let alsa_device = state.lock().unwrap().alsa_device.clone();
    let bpm         = state.lock().unwrap().bpm;

    let engine = AudioEngine::new(&alsa_device, bpm);
    let engine_reader = engine.lock().unwrap().reader(); // Extract the Lock-free Reader Handle
    
    {
        engine.lock().unwrap().start();
    }

    let brain  = Arc::new(Brain::new("http://127.0.0.1:5002"));
    let midi   = Arc::new(MidiRouter::new("AudioBox"));

    let win = ApplicationWindow::builder()
        .application(app)
        .title("TitanDAW")
        .default_width(1280)
        .default_height(820)
        .build();

    let css = gtk::CssProvider::new();
    css.load_from_data(
        b"window { background-color: #0d1117; }
          button { background: #161b22; color: #e6edf3; border: 1px solid #30363d;
                   border-radius: 4px; padding: 4px 14px; font-family: monospace; font-size: 12px; }
          button:hover { background: #1c2128; }
          button.rec { background: #f85149; color: white; border-color: #f85149; }
          button.rec:hover { background: #da3633; }
          button.active { border-color: #00ff9d; color: #00ff9d; }
          entry { background: #0a0e14; color: #e6edf3; border: 1px solid #30363d;
                  border-radius: 4px; padding: 4px 8px; font-family: monospace; font-size: 12px; }
          entry:focus { border-color: #00ff9d; }
          textview { background: #0a0e14; color: #e6edf3; font-family: monospace; font-size: 11px; }
          label { color: #7d8590; font-family: monospace; font-size: 11px; }"
    );
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let root = GBox::new(Orientation::Vertical, 0);

    let transport_canvas = DrawingArea::builder()
        .height_request(48)
        .hexpand(true)
        .build();
    {
        let state = state.clone();
        transport_canvas.set_draw_func(move |_, cr, w, _h| {
            let s = state.lock().unwrap();
            draw_transport(cr, w as f64, &s);
        });
    }

    let btn_row = GBox::new(Orientation::Horizontal, 6);
    btn_row.set_margin_start(12);
    btn_row.set_margin_end(12);
    btn_row.set_margin_top(6);
    btn_row.set_margin_bottom(6);

    macro_rules! btn {
        ($label:expr) => {{
            let b = Button::with_label($label);
            btn_row.append(&b);
            b
        }};
    }

    let btn_rewind  = btn!("⏮");
    let btn_play    = btn!("▶ Play");
    let btn_stop    = btn!("⏹ Stop");
    let btn_rec     = btn!("⏺ Rec");
    btn_rec.add_css_class("rec");

    let bpm_label = Label::new(Some("BPM"));
    let bpm_entry = Entry::builder()
        .text("140")
        .max_width_chars(6)
        .build();
    btn_row.append(&bpm_label);
    btn_row.append(&bpm_entry);

    let track_canvas = DrawingArea::builder()
        .height_request((RULER_H + MAX_TRACKS as f64 * TRACK_H) as i32)
        .hexpand(true)
        .build();
    {
        let state = state.clone();
        track_canvas.set_draw_func(move |_, cr, _w, _h| {
            let s = state.lock().unwrap();
            draw_tracks(cr, &s);
        });
    }

    let click = gtk::GestureClick::new();
    {
        let state   = state.clone();
        let engine  = engine.clone();
        let canvas  = track_canvas.clone();
        click.connect_pressed(move |_, _n, _x, y| {
            let row = ((y - RULER_H) / TRACK_H) as usize;
            if row < MAX_TRACKS {
                let mut s = state.lock().unwrap();
                s.selected_track = row;
                engine.lock().unwrap().arm(row as i32);
            }
            canvas.queue_draw();
        });
    }
    track_canvas.add_controller(click);

    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&track_canvas)
        .build();

    let bottom = GBox::new(Orientation::Horizontal, 0);
    bottom.set_height_request(220);

    let console_box = GBox::new(Orientation::Vertical, 4);
    console_box.set_width_request(480);
    console_box.set_margin_start(8);
    console_box.set_margin_top(8);

    let console_label = Label::new(Some("AI BRAIN CONSOLE"));
    console_label.set_halign(Align::Start);

    let console_buf  = TextBuffer::new(None::<&gtk::TextTagTable>);
    let console_view = TextView::builder()
        .buffer(&console_buf)
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::Word)
        .build();
    let console_scroll = ScrolledWindow::builder()
        .child(&console_view)
        .vexpand(true)
        .build();

    let input_row = GBox::new(Orientation::Horizontal, 4);
    let ai_entry  = Entry::builder()
        .placeholder_text("Ask Titan AI... (e.g. 'dark trap 140bpm')")
        .hexpand(true)
        .build();
    let ai_send = Button::with_label("Send");

    {
        let brain      = brain.clone();
        let buf        = console_buf.clone();
        let entry      = ai_entry.clone();
        ai_send.connect_clicked(move |_| {
            let text = entry.text().to_string();
            if text.is_empty() { return; }
            brain.chat(&text);
            buf.insert(&mut buf.end_iter(), &format!("[YOU] {}\n", text));
            entry.set_text("");
        });
    }

    {
        let btn = ai_send.clone();
        ai_entry.connect_activate(move |_| btn.emit_clicked());
    }

    input_row.append(&ai_entry);
    input_row.append(&ai_send);

    console_box.append(&console_label);
    console_box.append(&console_scroll);
    console_box.append(&input_row);

    let mixer_box = GBox::new(Orientation::Horizontal, 2);
    mixer_box.set_hexpand(true);
    mixer_box.set_margin_start(8);
    mixer_box.set_margin_top(8);

    for i in 0..8 {
        let strip = GBox::new(Orientation::Vertical, 2);
        strip.set_width_request(60);

        let lbl = Label::new(Some(&format!("T{:02}", i+1)));

        let fader = Scale::builder()
            .orientation(Orientation::Vertical)
            .value_pos(gtk::PositionType::Bottom)
            .draw_value(false)
            .inverted(true)
            .build();
        fader.set_range(0.0, 1.0);
        fader.set_value(0.8);
        fader.set_vexpand(true);

        {
            let engine = engine.clone();
            let state  = state.clone();
            fader.connect_value_changed(move |s| {
                let v = s.value() as f32;
                engine.lock().unwrap().set_volume(i as i32, v);
                state.lock().unwrap().volumes[i] = v;
            });
        }

        let mute_btn = Button::with_label("M");
        {
            let engine = engine.clone();
            let state  = state.clone();
            mute_btn.connect_clicked(move |b| {
                let mut s = state.lock().unwrap();
                s.muted[i] = !s.muted[i];
                engine.lock().unwrap().mute(i as i32, s.muted[i]);
                if s.muted[i] { b.add_css_class("active"); }
                else          { b.remove_css_class("active"); }
            });
        }

        strip.append(&lbl);
        strip.append(&fader);
        strip.append(&mute_btn);
        mixer_box.append(&strip);

        if i < 7 {
            mixer_box.append(&Separator::new(Orientation::Vertical));
        }
    }

    bottom.append(&console_box);
    bottom.append(&Separator::new(Orientation::Vertical));
    bottom.append(&mixer_box);

    let alert_canvas = DrawingArea::builder()
        .height_request(32)
        .hexpand(true)
        .build();
    {
        let state = state.clone();
        alert_canvas.set_draw_func(move |_, cr, w, _h| {
            let s = state.lock().unwrap();
            draw_alert_strip(cr, w as f64, &s.alerts);
        });
    }

    // ── Wire transport buttons ────────────────────────────────────────────────

    {
        let engine = engine.clone();
        let state  = state.clone();
        let canvas = track_canvas.clone();
        let tc     = transport_canvas.clone();
        btn_play.connect_clicked(move |_| {
            engine.lock().unwrap().play();
            state.lock().unwrap().is_playing = true;
            canvas.queue_draw();
            tc.queue_draw();
        });
    }
    {
        let engine = engine.clone();
        let state  = state.clone();
        let canvas = track_canvas.clone();
        let tc     = transport_canvas.clone();
        btn_stop.connect_clicked(move |_| {
            let mut eng = engine.lock().unwrap();
            eng.stop_playback();
            eng.record_stop();
            let mut s = state.lock().unwrap();
            s.is_playing   = false;
            s.is_recording = false;
            canvas.queue_draw();
            tc.queue_draw();
        });
    }
    {
        let engine = engine.clone();
        let state  = state.clone();
        let canvas = track_canvas.clone();
        let tc     = transport_canvas.clone();
        btn_rewind.connect_clicked(move |_| {
            engine.lock().unwrap().rewind();
            state.lock().unwrap().transport_sec = 0.0;
            canvas.queue_draw();
            tc.queue_draw();
        });
    }
    {
        let engine = engine.clone();
        let state  = state.clone();
        let canvas = track_canvas.clone();
        let tc     = transport_canvas.clone();
        btn_rec.connect_clicked(move |_| {
            let mut s = state.lock().unwrap();
            let tid = s.selected_track as i32;
            s.is_recording = true;
            s.is_playing   = true;
            drop(s);
            engine.lock().unwrap().record_start(tid);
            canvas.queue_draw();
            tc.queue_draw();
        });
    }
    {
        let state = state.clone();
        bpm_entry.connect_activate(move |e| {
            if let Ok(bpm) = e.text().parse::<f64>() {
                state.lock().unwrap().bpm = bpm.clamp(20.0, 300.0);
            }
        });
    }

    root.append(&transport_canvas);
    root.append(&btn_row);
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&scroll);
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&bottom);
    root.append(&alert_canvas);

    win.set_child(Some(&root));
    win.present();

    // ── 60fps Lock-Free + Damage-Tracked Render & MIDI Processing Loop ───────
    {
        let state            = state.clone();
        let engine_clone     = engine.clone();
        let brain            = brain.clone();
        let midi             = midi.clone();
        let track_canvas     = track_canvas.clone();
        let transport_canvas = transport_canvas.clone();
        let alert_canvas     = alert_canvas.clone();
        let console_buf      = console_buf.clone();
        
        let reader           = engine_reader;

        let mut prev_peaks_l   = [0.0f32; MAX_TRACKS];
        let mut prev_peaks_r   = [0.0f32; MAX_TRACKS];
        let mut prev_transport = -1.0f64;

        timeout_add_local(Duration::from_millis(16), move || {
            let mut s = state.lock().unwrap();
            let mut needs_repaint = false;

            // 1. Lock-Free Ingestion of Hardware MIDI Knobs (CC 7 & CC 10)
            while let Ok(ev) = midi.rx.try_recv() {
                match ev {
                    crate::midi::MidiEvent::CC { channel: _, cc, value } => {
                        let active_track = s.selected_track;
                        if active_track < MAX_TRACKS {
                            let mut eng = engine_clone.lock().unwrap();
                            if cc == 7 {
                                let vol = value as f32 / 127.0;
                                eng.set_volume(active_track as i32, vol);
                                s.volumes[active_track] = vol;
                                needs_repaint = true;
                            } else if cc == 10 {
                                let pan = ((value as f32 / 127.0) * 2.0) - 1.0;
                                eng.set_pan(active_track as i32, pan);
                                s.pans[active_track] = pan;
                                needs_repaint = true;
                            }
                        }
                    },
                    _ => {}
                }
            }

            // 2. Lock-Free Metric Evaluation with Frame-Throttling Thresholds
            {
                for i in 0..MAX_TRACKS {
                    let pk_l = reader.peak_l(i as i32);
                    let pk_r = reader.peak_r(i as i32);
                    
                    if (pk_l - prev_peaks_l[i]).abs() > 0.005 || (pk_r - prev_peaks_r[i]).abs() > 0.005 {
                        s.peaks_l[i] = pk_l;
                        s.peaks_r[i] = pk_r;
                        prev_peaks_l[i] = pk_l;
                        prev_peaks_r[i] = pk_r;
                        needs_repaint = true;
                    }
                }

                s.master_l = reader.master_peak_l();
                s.master_r = reader.master_peak_r();

                if s.is_playing {
                    s.transport_sec = reader.transport_frame() as f64 / 48000.0;
                    if (s.transport_sec - prev_transport).abs() > 0.001 {
                        prev_transport = s.transport_sec;
                        needs_repaint = true;
                    }
                }
            }

            // 3. Drain LLM Telemetry Notifications
            let alerts = brain.drain_alerts();
            if !alerts.is_empty() {
                for a in &alerts {
                    let line = format!("[TITAN AI] {}\n", a.message);
                    console_buf.insert(&mut console_buf.end_iter(), &line);
                    s.alerts.push((a.level.clone(), a.message.clone(), a.tag.clone()));
                    if s.alerts.len() > 32 { s.alerts.remove(0); }
                }
                alert_canvas.queue_draw();
            }

            drop(s);

            // 4. Fire Repaint Hooks Only If State Changes Exist (Saves CPU)
            if needs_repaint {
                track_canvas.queue_draw();
                transport_canvas.queue_draw();
            }

            glib::ControlFlow::Continue
        });
    }
}