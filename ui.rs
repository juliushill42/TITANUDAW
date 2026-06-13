// ui.rs — TitanDAW GTK4 production UI   JCH-2026-011
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GBox, Orientation, DrawingArea,
    Button, Label, Scale, Entry, ScrolledWindow, TextView, TextBuffer,
    Align, Separator, FileDialog,
};
use cairo::Context;
use glib::timeout_add_local;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::engine::AudioEngine;
use crate::brain::{Brain, AlertLevel};
use crate::midi::MidiRouter;
use crate::session::{SessionState, default_session_path, save as save_session, load as load_session};

const SURFACE: (f64,f64,f64) = (0.055, 0.071, 0.090);
const BORDER:  (f64,f64,f64) = (0.165, 0.188, 0.212);
const ACCENT:  (f64,f64,f64) = (0.0,   1.0,   0.616);
const ORANGE:  (f64,f64,f64) = (1.0,   0.420, 0.208);
const RED:     (f64,f64,f64) = (0.973, 0.318, 0.286);
const PURPLE:  (f64,f64,f64) = (0.737, 0.549, 1.0);
const TEXT:    (f64,f64,f64) = (0.902, 0.929, 0.953);
const MUTED:   (f64,f64,f64) = (0.490, 0.522, 0.565);
const YELLOW:  (f64,f64,f64) = (0.890, 0.702, 0.255);
const CYAN:    (f64,f64,f64) = (0.0,   0.878, 1.0);

const TRACK_H:    f64   = 68.0;
const TRACK_W:    f64   = 980.0;
const RULER_H:    f64   = 26.0;
const HDR_W:      f64   = 200.0;
const MAX_TRACKS: usize = 16;
const SR:         f64   = 48000.0;

fn sc(cr: &Context, (r,g,b): (f64,f64,f64))          { cr.set_source_rgb(r,g,b); }
fn sca(cr: &Context, (r,g,b): (f64,f64,f64), a: f64) { cr.set_source_rgba(r,g,b,a); }

#[derive(Clone)]
pub struct LoopRegion { pub start_sec: f64, pub end_sec: f64, pub active: bool }

#[derive(Clone)]
struct UiState {
    selected:      usize,
    muted:         [bool; MAX_TRACKS],
    soloed:        [bool; MAX_TRACKS],
    volumes:       [f32;  MAX_TRACKS],
    pans:          [f32;  MAX_TRACKS],
    pitches:       [f32;  MAX_TRACKS],
    track_names:   [String; MAX_TRACKS],
    bpm:           f64,
    playing:       bool,
    recording:     bool,
    transport:     f64,
    peaks_l:       [f32; MAX_TRACKS],
    peaks_r:       [f32; MAX_TRACKS],
    master_l:      f32,
    master_r:      f32,
    alerts:        Vec<(AlertLevel, String, String)>,
    alsa_device:   String,
    buffer_frames: i32,
    metronome:     bool,
    loop_r:        LoopRegion,
    wav_paths:     [String; MAX_TRACKS],
}

impl Default for UiState {
    fn default() -> Self {
        const EMPTY: String = String::new();
        Self {
            selected: 0,
            muted:    [false; MAX_TRACKS],
            soloed:   [false; MAX_TRACKS],
            volumes:  [0.8;   MAX_TRACKS],
            pans:     [0.0;   MAX_TRACKS],
            pitches:  [1.0;   MAX_TRACKS],
            track_names: std::array::from_fn(|i| format!("Track {:02}", i+1)),
            bpm: 140.0, playing: false, recording: false, transport: 0.0,
            peaks_l: [0.0; MAX_TRACKS], peaks_r: [0.0; MAX_TRACKS],
            master_l: 0.0, master_r: 0.0,
            alerts: Vec::new(),
            alsa_device:   "hw:1,0".into(),
            buffer_frames: 256,
            metronome:     false,
            loop_r: LoopRegion { start_sec: 0.0, end_sec: 8.0, active: false },
            wav_paths: [EMPTY; MAX_TRACKS],
        }
    }
}

fn draw_vu(cr: &Context, x: f64, y: f64, w: f64, h: f64, l: f32, r: f32) {
    let half = h / 2.0 - 1.0;
    for (i, pk) in [l,r].iter().enumerate() {
        let by = y + i as f64 * (half + 2.0);
        sca(cr, BORDER, 0.7); cr.rectangle(x, by, w, half); cr.fill().ok();
        let fw = (w * *pk as f64).min(w);
        let col = if *pk > 0.9 { RED } else if *pk > 0.7 { YELLOW } else { ACCENT };
        sc(cr, col); cr.rectangle(x, by, fw, half); cr.fill().ok();
    }
}

fn draw_transport(cr: &Context, w: f64, s: &UiState) {
    sc(cr, SURFACE); cr.rectangle(0.0,0.0,w,52.0); cr.fill().ok();
    sc(cr, BORDER);  cr.set_line_width(1.0);
    cr.move_to(0.0,51.5); cr.line_to(w,51.5); cr.stroke().ok();
    sc(cr, ACCENT); cr.set_font_size(22.0);
    cr.move_to(16.0,36.0);
    cr.show_text(&format!("{:.1} BPM", s.bpm)).ok();
    let sec = s.transport as u64;
    let ms  = ((s.transport - sec as f64)*1000.0) as u64;
    sc(cr, TEXT); cr.set_font_size(13.0);
    cr.move_to(170.0,33.0);
    cr.show_text(&format!("{:02}:{:02}.{:03}", sec/60, sec%60, ms)).ok();
    if s.metronome { sc(cr, CYAN); cr.set_font_size(10.0); cr.move_to(310.0,33.0); cr.show_text("♩ CLICK").ok(); }
    if s.loop_r.active {
        sc(cr, PURPLE); cr.set_font_size(10.0); cr.move_to(390.0,33.0);
        cr.show_text(&format!("⟳ {:.1}–{:.1}s", s.loop_r.start_sec, s.loop_r.end_sec)).ok();
    }
    if s.recording {
        sc(cr, RED); cr.arc(w/2.0,26.0,9.0,0.0,std::f64::consts::TAU); cr.fill().ok();
        sc(cr, TEXT); cr.set_font_size(11.0); cr.move_to(w/2.0+16.0,30.0); cr.show_text("REC").ok();
    } else if s.playing {
        sc(cr, ACCENT); cr.set_font_size(11.0); cr.move_to(w/2.0+16.0,30.0); cr.show_text("PLAYING").ok();
    }
    let vx = w - 150.0;
    draw_vu(cr, vx, 10.0, 120.0, 16.0, s.master_l, s.master_r);
    sc(cr, MUTED); cr.set_font_size(9.0); cr.move_to(vx,44.0); cr.show_text("MASTER").ok();
}

fn draw_tracks(cr: &Context, s: &UiState) {
    sc(cr, SURFACE); cr.rectangle(HDR_W,0.0,TRACK_W,RULER_H); cr.fill().ok();
    sc(cr, MUTED); cr.set_font_size(9.0); cr.set_line_width(0.5);
    let beats_per_sec = s.bpm / 60.0;
    let px_per_beat   = 80.0;
    for beat in 0..128i32 {
        let rx = HDR_W + beat as f64 * px_per_beat;
        if rx > HDR_W + TRACK_W { break; }
        cr.move_to(rx, 0.0);
        cr.line_to(rx, if beat%4==0 { RULER_H } else { RULER_H*0.5 });
        cr.stroke().ok();
        if beat%4==0 { cr.move_to(rx+3.0, RULER_H-5.0); cr.show_text(&format!("{}",beat/4+1)).ok(); }
    }
    if s.loop_r.active {
        let lx = HDR_W + s.loop_r.start_sec * beats_per_sec * px_per_beat;
        let lw = (s.loop_r.end_sec - s.loop_r.start_sec) * beats_per_sec * px_per_beat;
        sca(cr, PURPLE, 0.12); cr.rectangle(lx, 0.0, lw, RULER_H + MAX_TRACKS as f64*TRACK_H); cr.fill().ok();
        sc(cr, PURPLE); cr.set_line_width(1.0);
        cr.move_to(lx,0.0); cr.line_to(lx, RULER_H+MAX_TRACKS as f64*TRACK_H); cr.stroke().ok();
        cr.move_to(lx+lw,0.0); cr.line_to(lx+lw, RULER_H+MAX_TRACKS as f64*TRACK_H); cr.stroke().ok();
    }
    let cx = HDR_W + s.transport * beats_per_sec * px_per_beat;
    sc(cr, ORANGE); cr.set_line_width(1.5);
    cr.move_to(cx,0.0); cr.line_to(cx, RULER_H+MAX_TRACKS as f64*TRACK_H); cr.stroke().ok();
    for i in 0..MAX_TRACKS {
        let ty = RULER_H + i as f64 * TRACK_H;
        let selected = i == s.selected;
        let hc = if selected { (0.10,0.16,0.22) } else { SURFACE };
        sc(cr, hc); cr.rectangle(0.0,ty,HDR_W,TRACK_H); cr.fill().ok();
        sc(cr, if selected { ACCENT } else { TEXT });
        cr.set_font_size(11.0); cr.move_to(8.0, ty+18.0);
        let name = &s.track_names[i];
        let short = if name.len()>18 { &name[..18] } else { name.as_str() };
        cr.show_text(short).ok();
        draw_vu(cr, 8.0, ty+26.0, 72.0, 22.0, s.peaks_l[i], s.peaks_r[i]);
        if s.muted[i]  { sc(cr, YELLOW); cr.set_font_size(8.0); cr.move_to(88.0,ty+20.0); cr.show_text("M").ok(); }
        if s.soloed[i] { sc(cr, CYAN);   cr.set_font_size(8.0); cr.move_to(100.0,ty+20.0); cr.show_text("S").ok(); }
        let px = HDR_W - 28.0 + s.pans[i] as f64*12.0;
        sc(cr, PURPLE); cr.arc(px, ty+14.0, 3.5, 0.0, std::f64::consts::TAU); cr.fill().ok();
        let vh = TRACK_H - 14.0; let vf = vh * s.volumes[i] as f64;
        sca(cr, BORDER, 0.9); cr.rectangle(HDR_W-10.0, ty+7.0, 6.0, vh); cr.fill().ok();
        sc(cr, ACCENT);       cr.rectangle(HDR_W-10.0, ty+7.0+(vh-vf), 6.0, vf); cr.fill().ok();
        let pitch = s.pitches[i];
        let pc = if pitch > 1.0 { ACCENT } else if pitch < 1.0 { ORANGE } else { MUTED };
        sc(cr, pc); cr.set_font_size(8.0); cr.move_to(88.0, ty+36.0);
        cr.show_text(&format!("{:.2}x", pitch)).ok();
        let rc = if selected { (0.082,0.114,0.141) } else { (0.059,0.078,0.098) };
        sc(cr, rc); cr.rectangle(HDR_W, ty, TRACK_W, TRACK_H-1.0); cr.fill().ok();
        let has_audio = s.peaks_l[i] > 0.001 || s.peaks_r[i] > 0.001 || !s.wav_paths[i].is_empty();
        if has_audio {
            sca(cr, ACCENT, 0.18); cr.rectangle(HDR_W+2.0, ty+4.0, 300.0, TRACK_H-8.0); cr.fill().ok();
            sca(cr, ACCENT, 0.45); cr.set_line_width(1.0);
            let mid = ty + TRACK_H/2.0;
            let amp_scale = (s.peaks_l[i] + s.peaks_r[i]) / 2.0;
            for x in (0..300usize).step_by(3) {
                let amp = amp_scale as f64 * 18.0 * (x as f64 * 0.25).sin().abs() + 1.0;
                cr.move_to(HDR_W+2.0+x as f64, mid-amp);
                cr.line_to(HDR_W+2.0+x as f64, mid+amp);
                cr.stroke().ok();
            }
            if !s.wav_paths[i].is_empty() {
                let fname: &str = s.wav_paths[i].split('/').last().unwrap_or("");
                let short_f = if fname.len()>36 { &fname[..36] } else { fname };
                sc(cr, TEXT); cr.set_font_size(9.0);
                cr.move_to(HDR_W+8.0, ty+14.0); cr.show_text(short_f).ok();
            }
        }
        sc(cr, BORDER); cr.set_line_width(0.5);
        cr.move_to(0.0, ty+TRACK_H-0.5); cr.line_to(HDR_W+TRACK_W, ty+TRACK_H-0.5); cr.stroke().ok();
    }
}

fn draw_alert(cr: &Context, w: f64, alerts: &[(AlertLevel, String, String)]) {
    sc(cr, SURFACE); cr.rectangle(0.0,0.0,w,34.0); cr.fill().ok();
    if let Some((lvl, msg, tag)) = alerts.last() {
        let col = match lvl { AlertLevel::Critical=>RED, AlertLevel::Warn=>YELLOW, AlertLevel::Info=>ACCENT };
        sc(cr, col); cr.rectangle(0.0,0.0,4.0,34.0); cr.fill().ok();
        sca(cr, col, 0.18); cr.rectangle(8.0,7.0,52.0,20.0); cr.fill().ok();
        sc(cr, col); cr.set_font_size(9.0); cr.move_to(12.0,22.0); cr.show_text(&tag.to_uppercase()).ok();
        sc(cr, TEXT); cr.set_font_size(11.0); cr.move_to(70.0,22.0);
        let s = if msg.len()>130 { &msg[..130] } else { msg.as_str() };
        cr.show_text(s).ok();
    } else {
        sc(cr, MUTED); cr.set_font_size(10.0); cr.move_to(10.0,22.0);
        cr.show_text("TITAN AI — STANDBY  |  Ask anything: 'make a dark trap beat', 'fix my mix', 'help me record vocals'").ok();
    }
}

pub fn build_and_show(app: &Application) {
    let session_path = default_session_path();
    let saved = load_session(&session_path).ok();
    let mut init_state = UiState::default();
    if let Some(ref ss) = saved {
        init_state.bpm           = ss.bpm;
        init_state.alsa_device   = ss.alsa_device.clone();
        init_state.buffer_frames = ss.buffer_frames;
        init_state.metronome     = ss.metronome;
        init_state.loop_r        = LoopRegion {
            start_sec: ss.loop_start_sec,
            end_sec:   ss.loop_end_sec,
            active:    ss.loop_active,
        };
        for i in 0..MAX_TRACKS.min(ss.volumes.len()) {
            init_state.volumes[i]   = ss.volumes[i];
            init_state.pans[i]      = ss.pans[i];
            init_state.pitches[i]   = ss.pitches[i];
            init_state.muted[i]     = ss.muted[i];
            init_state.soloed[i]    = ss.soloed[i];
            init_state.wav_paths[i] = ss.wav_paths.get(i).cloned().unwrap_or_default();
        }
        for i in 0..MAX_TRACKS.min(ss.track_names.len()) {
            init_state.track_names[i] = ss.track_names[i].clone();
        }
    }

    let devices = AudioEngine::list_devices();
    if let Some(dev) = devices.iter().find(|d| {
        let l = d.to_lowercase();
        l.contains("usb") || l.contains("audiobox") || l.contains("hw:1")
    }) { init_state.alsa_device = dev.clone(); }

    let state  = Arc::new(Mutex::new(init_state.clone()));
    let engine = AudioEngine::new(&init_state.alsa_device, init_state.bpm, init_state.buffer_frames);

    {
        let mut eng = engine.lock().unwrap();
        for i in 0..MAX_TRACKS {
            eng.set_volume(i as i32, init_state.volumes[i]);
            eng.set_pan(i as i32, init_state.pans[i]);
            eng.set_pitch(i as i32, init_state.pitches[i]);
            eng.mute(i as i32, init_state.muted[i]);
            eng.solo(i as i32, init_state.soloed[i]);
            eng.set_track_name(i as i32, &init_state.track_names[i]);
        }
        eng.set_metronome(init_state.metronome);
        eng.set_loop(
            (init_state.loop_r.start_sec * SR) as i64,
            (init_state.loop_r.end_sec   * SR) as i64,
            init_state.loop_r.active,
        );
        for i in 0..MAX_TRACKS {
            if !init_state.wav_paths[i].is_empty() {
                eng.load_wav(i as i32, &init_state.wav_paths[i]);
            }
        }
    }

    let brain = Arc::new(Brain::new("http://127.0.0.1:5002"));
    let midi  = Arc::new(MidiRouter::new("AudioBox"));

    let win = ApplicationWindow::builder()
        .application(app).title("TitanDAW v2")
        .default_width(1440).default_height(900).build();

    let css = gtk4::CssProvider::new();
    css.load_from_data(
        "window{background:#0d1117}
         button{background:#161b22;color:#e6edf3;border:1px solid #30363d;border-radius:4px;padding:4px 12px;font-family:monospace;font-size:11px}
         button:hover{background:#1c2128}
         button.rec{background:#f85149;color:#fff;border-color:#f85149}
         button.rec:hover{background:#da3633}
         button.active{border-color:#00ff9d;color:#00ff9d}
         button.solo{border-color:#00e1ff;color:#00e1ff}
         entry{background:#0a0e14;color:#e6edf3;border:1px solid #30363d;border-radius:4px;padding:4px 8px;font-family:monospace;font-size:11px}
         entry:focus{border-color:#00ff9d}
         textview{background:#0a0e14;color:#c9d1d9;font-family:monospace;font-size:11px}
         label{color:#7d8590;font-family:monospace;font-size:11px}
         scale trough{background:#1c2128;border-radius:2px}
         scale highlight{background:#00ff9d;border-radius:2px}"
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(), &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);

    let root = GBox::new(Orientation::Vertical, 0);

    let tc = DrawingArea::builder().height_request(52).hexpand(true).build();
    { let s=state.clone(); tc.set_draw_func(move|_,cr,w,_|{ draw_transport(cr,w as f64,&s.lock().unwrap()); }); }

    let br = GBox::new(Orientation::Horizontal, 5);
    br.set_margin_start(10); br.set_margin_end(10);
    br.set_margin_top(5); br.set_margin_bottom(5);

    macro_rules! btn { ($l:expr) => {{ let b=Button::with_label($l); br.append(&b); b }}; }

    let btn_rew  = btn!("⏮");
    let btn_play = btn!("▶ Play");
    let btn_stop = btn!("⏹ Stop");
    let btn_rec  = btn!("⏺ Rec"); btn_rec.add_css_class("rec");
    let btn_click= btn!("♩ Click");
    let btn_loop = btn!("⟳ Loop");

    br.append(&Separator::new(Orientation::Vertical));
    let _bpm_lbl = Label::new(Some("BPM")); br.append(&_bpm_lbl);
    let bpm_ent = Entry::builder().text(&format!("{:.0}", init_state.bpm)).max_width_chars(5).build();
    br.append(&bpm_ent);

    br.append(&Separator::new(Orientation::Vertical));
    let btn_import  = btn!("📂 Import WAV");
    let btn_save    = btn!("💾 Save");
    let btn_mix     = btn!("⚡ Mix Export");
    let btn_stems   = btn!("🎚 Stem Export");
    br.append(&Separator::new(Orientation::Vertical));
    let ai_quick = Entry::builder()
        .placeholder_text("Ask Titan AI…")
        .hexpand(true).build();
    br.append(&ai_quick);
    let btn_ai_send = btn!("→ Ask");

    let tch = (RULER_H + MAX_TRACKS as f64 * TRACK_H) as i32;
    let track_canvas = DrawingArea::builder().height_request(tch).hexpand(true).build();
    { let s=state.clone(); track_canvas.set_draw_func(move|_,cr,_,_|{ draw_tracks(cr,&s.lock().unwrap()); }); }

    let click_gest = gtk4::GestureClick::new();
    { let s=state.clone(); let eng=engine.clone(); let tc2=track_canvas.clone();
      click_gest.connect_pressed(move|_,_,_x,y|{
          let row = ((y - RULER_H) / TRACK_H) as usize;
          if row < MAX_TRACKS {
              let mut st = s.lock().unwrap();
              st.selected = row;
              drop(st);
              eng.lock().unwrap().arm(row as i32);
          }
          tc2.queue_draw();
      }); }
    track_canvas.add_controller(click_gest);

    let scroll = ScrolledWindow::builder().hexpand(true).vexpand(true).child(&track_canvas).build();

    let bottom = GBox::new(Orientation::Horizontal, 0);
    bottom.set_height_request(240);

    let con_box  = GBox::new(Orientation::Vertical, 4);
    con_box.set_width_request(500); con_box.set_margin_start(8); con_box.set_margin_top(8);
    let con_lbl  = Label::new(Some("TITAN AI PRODUCER BRAIN")); con_lbl.set_halign(Align::Start);
    let con_buf  = TextBuffer::new(None::<&gtk4::TextTagTable>);
    let con_view = TextView::builder().buffer(&con_buf).editable(false).cursor_visible(false)
        .wrap_mode(gtk4::WrapMode::Word).build();
    let con_scr  = ScrolledWindow::builder().child(&con_view).vexpand(true).build();
    let in_row   = GBox::new(Orientation::Horizontal, 4);
    let ai_ent   = Entry::builder().placeholder_text("Ask anything…").hexpand(true).build();
    let ai_btn   = Button::with_label("Send");
    in_row.append(&ai_ent); in_row.append(&ai_btn);
    con_box.append(&con_lbl); con_box.append(&con_scr); con_box.append(&in_row);

    let make_brain_sender = |brain: Arc<Brain>, buf: TextBuffer, entry: Entry| {
        move |text: String| {
            if text.is_empty() { return; }
            brain.chat(&text);
            buf.insert(&mut buf.end_iter(), &format!("[YOU] {}\n", text));
            entry.set_text("");
        }
    };

    { let brain=brain.clone(); let buf=con_buf.clone(); let ent=ai_ent.clone();
      ai_btn.connect_clicked(move|_|{
          let t=ent.text().to_string();
          let sender=make_brain_sender(brain.clone(),buf.clone(),ent.clone());
          sender(t);
      }); }
    { let btn=ai_btn.clone(); ai_ent.connect_activate(move|_|btn.emit_clicked()); }

    { let brain=brain.clone(); let buf=con_buf.clone(); let ent=ai_quick.clone();
      btn_ai_send.connect_clicked(move|_|{
          let t=ent.text().to_string();
          let sender=make_brain_sender(brain.clone(),buf.clone(),ent.clone());
          sender(t);
      }); }
    { let btn=btn_ai_send.clone(); ai_quick.connect_activate(move|_|btn.emit_clicked()); }

    let mix_outer = ScrolledWindow::builder().hexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never).build();
    let mix_box = GBox::new(Orientation::Horizontal, 2);
    mix_box.set_margin_start(8); mix_box.set_margin_top(8);

    for i in 0..MAX_TRACKS {
        let strip = GBox::new(Orientation::Vertical, 2);
        strip.set_width_request(56);
        let lbl = Label::new(Some(&init_state.track_names[i][..init_state.track_names[i].len().min(5)]));
        let vol_f = Scale::builder().orientation(Orientation::Vertical)
            .draw_value(false).inverted(true).build();
        vol_f.set_range(0.0, 1.0); vol_f.set_value(init_state.volumes[i] as f64);
        vol_f.set_vexpand(true);
        { let eng=engine.clone(); let s=state.clone();
          vol_f.connect_value_changed(move|sl|{
              let v=sl.value() as f32;
              eng.lock().unwrap().set_volume(i as i32,v);
              s.lock().unwrap().volumes[i]=v;
          }); }
        let pit_f = Scale::builder().orientation(Orientation::Horizontal).draw_value(false).build();
        pit_f.set_range(0.5, 2.0); pit_f.set_value(init_state.pitches[i] as f64);
        { let eng=engine.clone(); let s=state.clone();
          pit_f.connect_value_changed(move|sl|{
              let p=sl.value() as f32;
              eng.lock().unwrap().set_pitch(i as i32,p);
              s.lock().unwrap().pitches[i]=p;
          }); }
        let mute_b = Button::with_label("M");
        if init_state.muted[i] { mute_b.add_css_class("active"); }
        { let eng=engine.clone(); let s=state.clone(); let b=mute_b.clone();
          mute_b.connect_clicked(move|_|{
              let mut st=s.lock().unwrap();
              st.muted[i]=!st.muted[i];
              let m=st.muted[i]; drop(st);
              eng.lock().unwrap().mute(i as i32,m);
              if m { b.add_css_class("active"); } else { b.remove_css_class("active"); }
          }); }
        let solo_b = Button::with_label("S");
        if init_state.soloed[i] { solo_b.add_css_class("solo"); }
        { let eng=engine.clone(); let s=state.clone(); let b=solo_b.clone();
          solo_b.connect_clicked(move|_|{
              let mut st=s.lock().unwrap();
              st.soloed[i]=!st.soloed[i];
              let solo=st.soloed[i]; drop(st);
              eng.lock().unwrap().solo(i as i32,solo);
              if solo { b.add_css_class("solo"); } else { b.remove_css_class("solo"); }
          }); }
        strip.append(&lbl); strip.append(&vol_f);
        strip.append(&Label::new(Some("PTH"))); strip.append(&pit_f);
        let ms_row = GBox::new(Orientation::Horizontal, 2);
        ms_row.append(&mute_b); ms_row.append(&solo_b);
        strip.append(&ms_row);
        mix_box.append(&strip);
        if i < MAX_TRACKS-1 { mix_box.append(&Separator::new(Orientation::Vertical)); }
    }
    mix_outer.set_child(Some(&mix_box));
    bottom.append(&con_box);
    bottom.append(&Separator::new(Orientation::Vertical));
    bottom.append(&mix_outer);

    let alert_c = DrawingArea::builder().height_request(34).hexpand(true).build();
    { let s=state.clone(); alert_c.set_draw_func(move|_,cr,w,_|{ draw_alert(cr,w as f64,&s.lock().unwrap().alerts); }); }

    { let e=engine.clone(); let s=state.clone(); let tc_=tc.clone(); let trc=track_canvas.clone();
      btn_play.connect_clicked(move|_|{ e.lock().unwrap().play(); s.lock().unwrap().playing=true; tc_.queue_draw(); trc.queue_draw(); }); }

    { let e=engine.clone(); let s=state.clone(); let tc_=tc.clone(); let trc=track_canvas.clone();
      btn_stop.connect_clicked(move|_|{
          let mut eng=e.lock().unwrap(); eng.stop_playback();
          let st_idx = s.lock().unwrap().selected as i32;
          eng.record_stop(st_idx); drop(eng);
          let mut st=s.lock().unwrap(); st.playing=false; st.recording=false;
          tc_.queue_draw(); trc.queue_draw();
      }); }

    { let e=engine.clone(); let s=state.clone(); let tc_=tc.clone(); let trc=track_canvas.clone();
      btn_rew.connect_clicked(move|_|{ e.lock().unwrap().rewind(); s.lock().unwrap().transport=0.0; tc_.queue_draw(); trc.queue_draw(); }); }

    { let e=engine.clone(); let s=state.clone(); let tc_=tc.clone(); let trc=track_canvas.clone();
      btn_rec.connect_clicked(move|_|{
          let mut st=s.lock().unwrap(); let tid=st.selected as i32;
          st.recording=true; st.playing=true; drop(st);
          e.lock().unwrap().record_start(tid);
          tc_.queue_draw(); trc.queue_draw();
      }); }

    { let e=engine.clone(); let s=state.clone(); let tc_=tc.clone(); let b=btn_click.clone();
      btn_click.connect_clicked(move|_|{
          let mut st=s.lock().unwrap(); st.metronome=!st.metronome; let m=st.metronome; drop(st);
          e.lock().unwrap().set_metronome(m);
          if m { b.add_css_class("active"); } else { b.remove_css_class("active"); }
          tc_.queue_draw();
      }); }

    { let e=engine.clone(); let s=state.clone(); let trc=track_canvas.clone(); let b=btn_loop.clone();
      btn_loop.connect_clicked(move|_|{
          let mut st=s.lock().unwrap();
          st.loop_r.active=!st.loop_r.active;
          let (ls,le,la)=(st.loop_r.start_sec,st.loop_r.end_sec,st.loop_r.active); drop(st);
          e.lock().unwrap().set_loop((ls*SR) as i64,(le*SR) as i64,la);
          if la { b.add_css_class("active"); } else { b.remove_css_class("active"); }
          trc.queue_draw();
      }); }

    { let e=engine.clone(); let s=state.clone();
      bpm_ent.connect_activate(move|en|{
          if let Ok(bpm)=en.text().parse::<f64>() {
              let bpm=bpm.clamp(20.0,300.0);
              s.lock().unwrap().bpm=bpm;
              e.lock().unwrap().set_bpm(bpm);
          }
      }); }

    { let win_=win.clone(); let e=engine.clone(); let s=state.clone();
      let trc=track_canvas.clone(); let buf=con_buf.clone();
      btn_import.connect_clicked(move|_|{
          let dlg=FileDialog::builder().title("Import WAV into selected track").build();
          let s_=s.clone(); let e_=e.clone(); let trc_=trc.clone(); let buf_=buf.clone();
          dlg.open(Some(&win_), None::<&gtk4::gio::Cancellable>, move|res|{
              if let Ok(file)=res {
                  if let Some(path)=file.path() {
                      let ps=path.to_string_lossy().to_string();
                      let mut st=s_.lock().unwrap(); let tid=st.selected as i32;
                      let frames=e_.lock().unwrap().load_wav(tid,&ps);
                      let sel=st.selected; st.wav_paths[sel]=ps.clone();
                      let msg=if frames>0 {
                          format!("Loaded '{}' → T{:02} ({:.1}s)", ps.split('/').last().unwrap_or(&ps), tid+1, frames as f64/SR)
                      } else { format!("Failed to load: {}", ps) };
                      st.alerts.push((AlertLevel::Info, msg.clone(), "Import".into()));
                      if st.alerts.len()>64 { st.alerts.remove(0); }
                      drop(st);
                      buf_.insert(&mut buf_.end_iter(), &format!("[IMPORT] {}\n", msg));
                      trc_.queue_draw();
                  }
              }
          });
      }); }

    { let s=state.clone(); let buf=con_buf.clone(); let sp=session_path.clone();
      btn_save.connect_clicked(move|_|{
          let st=s.lock().unwrap();
          let ss=SessionState {
              version:        2,
              bpm:            st.bpm,
              volumes:        st.volumes.to_vec(),
              pans:           st.pans.to_vec(),
              pitches:        st.pitches.to_vec(),
              muted:          st.muted.to_vec(),
              soloed:         st.soloed.to_vec(),
              track_names:    st.track_names.to_vec(),
              loop_start_sec: st.loop_r.start_sec,
              loop_end_sec:   st.loop_r.end_sec,
              loop_active:    st.loop_r.active,
              metronome:      st.metronome,
              alsa_device:    st.alsa_device.clone(),
              buffer_frames:  st.buffer_frames,
              wav_paths:      st.wav_paths.to_vec(),
          };
          drop(st);
          match save_session(&ss, &sp) {
              Ok(_)  => { buf.insert(&mut buf.end_iter(), "[TITAN] Session saved.\n"); }
              Err(e) => { buf.insert(&mut buf.end_iter(), &format!("[TITAN] Save failed: {e}\n")); }
          }
      }); }

    { let win_=win.clone(); let e=engine.clone(); let s=state.clone(); let buf=con_buf.clone();
      btn_mix.connect_clicked(move|_|{
          let dlg=FileDialog::builder().title("Export Full Mixdown").initial_name("TitanDAW_Mix.wav").build();
          let s_=s.clone(); let e_=e.clone(); let buf_=buf.clone();
          dlg.save(Some(&win_), None::<&gtk4::gio::Cancellable>, move|res|{
              if let Ok(file)=res {
                  if let Some(path)=file.path() {
                      let ps=path.to_string_lossy().to_string();
                      let status=e_.lock().unwrap().export_wav(-1,&ps);
                      let mut st=s_.lock().unwrap();
                      let msg=if status==0 { format!("Mix exported: {}",ps) } else { "Mix export failed".into() };
                      let lvl=if status==0 { AlertLevel::Info } else { AlertLevel::Critical };
                      st.alerts.push((lvl,msg.clone(),"Export".into()));
                      drop(st);
                      buf_.insert(&mut buf_.end_iter(),&format!("[EXPORT] {}\n",msg));
                  }
              }
          });
      }); }

    { let win_=win.clone(); let e=engine.clone(); let s=state.clone(); let buf=con_buf.clone();
      btn_stems.connect_clicked(move|_|{
          let dlg=FileDialog::builder().title("Choose stem export folder").build();
          let s_=s.clone(); let e_=e.clone(); let buf_=buf.clone();
          dlg.open(Some(&win_), None::<&gtk4::gio::Cancellable>, move|res|{
              if let Ok(file)=res {
                  if let Some(path)=file.path() {
                      let dir=path.parent().map(|p|p.to_string_lossy().to_string()).unwrap_or_default();
                      let names=s_.lock().unwrap().track_names.clone();
                      let mut ok_count=0;
                      for i in 0..MAX_TRACKS {
                          let stem_path=format!("{}/Stem_{:02}_{}.wav",dir,i+1,names[i].replace(' ',"_"));
                          if e_.lock().unwrap().export_wav(i as i32,&stem_path)==0 { ok_count+=1; }
                      }
                      let msg=format!("Exported {ok_count} stems to {dir}");
                      buf_.insert(&mut buf_.end_iter(),&format!("[STEMS] {}\n",msg));
                  }
              }
          });
      }); }

    root.append(&tc);
    root.append(&br);
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&scroll);
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&bottom);
    root.append(&alert_c);
    win.set_child(Some(&root));

    {
        let s=state.clone(); let eng=engine.clone(); let brain=brain.clone();
        let midi=midi.clone(); let trc=track_canvas.clone(); let tc_=tc.clone();
        let ac=alert_c.clone(); let buf=con_buf.clone();
        let mut pp_l=[0.0f32;MAX_TRACKS]; let mut pp_r=[0.0f32;MAX_TRACKS];
        let mut prev_t=-1.0f64;

        timeout_add_local(Duration::from_millis(16), move || {
            let mut st=s.lock().unwrap();
            let mut dirty=false;

            while let Ok(ev)=midi.rx.try_recv() {
                if let crate::midi::MidiEvent::CC{cc,value,..}=ev {
                    let tid=st.selected as i32;
                    let mut eng2=eng.lock().unwrap();
                    match cc {
                        7  => { let v=value as f32/127.0; eng2.set_volume(tid,v); let sel=st.selected; st.volumes[sel]=v; dirty=true; }
                        10 => { let p=(value as f32/127.0)*2.0-1.0; eng2.set_pan(tid,p); let sel=st.selected; st.pans[sel]=p; dirty=true; }
                        _ => {}
                    }
                }
            }

            { let eng2=eng.lock().unwrap();
              for i in 0..MAX_TRACKS {
                  let (pl,pr)=(eng2.peak_l(i as i32),eng2.peak_r(i as i32));
                  if (pl-pp_l[i]).abs()>0.004 || (pr-pp_r[i]).abs()>0.004 {
                      st.peaks_l[i]=pl; st.peaks_r[i]=pr;
                      pp_l[i]=pl; pp_r[i]=pr; dirty=true;
                  }
              }
              st.master_l=eng2.master_peak_l(); st.master_r=eng2.master_peak_r();
              if st.playing {
                  let t=eng2.transport_frame() as f64/SR;
                  if (t-prev_t).abs()>0.0008 { st.transport=t; prev_t=t; dirty=true; }
              }
            }

            let alerts=brain.drain_alerts();
            if !alerts.is_empty() {
                for a in &alerts {
                    buf.insert(&mut buf.end_iter(),&format!("[TITAN AI] {}\n",a.message));
                    st.alerts.push((a.level.clone(),a.message.clone(),a.tag.clone()));
                    if st.alerts.len()>64 { st.alerts.remove(0); }
                }
                ac.queue_draw();
            }
            drop(st);
            if dirty { trc.queue_draw(); tc_.queue_draw(); }
            glib::ControlFlow::Continue
        });
    }

    win.present();
}
