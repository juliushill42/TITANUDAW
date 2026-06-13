// content_ui.rs — GTK4 Content Panel   JCH-2026-011
use gtk4::prelude::*;
use gtk4::{
    Box as GBox, Orientation, Button, Entry, Label,
    DrawingArea, ScrolledWindow, TextView, TextBuffer,
    Align, Separator, SpinButton, Adjustment, Window, DropDown, StringList,
};
use cairo::Context;
use glib::timeout_add_local;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::sync::mpsc;

const CONTENT_URL: &str = "http://127.0.0.1:6000";
const SURFACE:(f64,f64,f64) = (0.086,0.106,0.133);
const ACCENT: (f64,f64,f64) = (0.0,  1.0,  0.616);
const TEXT:   (f64,f64,f64) = (0.902,0.929,0.953);
const MUTED:  (f64,f64,f64) = (0.490,0.522,0.565);

#[derive(Default,Clone)]
struct ContentState { lines: Vec<String>, active_idx: usize, auto_scroll: bool }

fn sc(cr:&Context,(r,g,b):(f64,f64,f64)){cr.set_source_rgb(r,g,b);}
fn sca(cr:&Context,(r,g,b):(f64,f64,f64),a:f64){cr.set_source_rgba(r,g,b,a);}

fn draw_teleprompter(cr:&Context,w:f64,h:f64,s:&ContentState){
    cr.set_source_rgb(0.0,0.0,0.0); cr.rectangle(0.0,0.0,w,h); cr.fill().ok();
    if s.lines.is_empty(){
        sc(cr,MUTED); cr.set_font_size(18.0); cr.move_to(w/2.0-120.0,h/2.0);
        cr.show_text("TITAN TELEPROMPTER — LOAD LYRICS").ok(); return;
    }
    let lh=56.0; let vis=(h/lh) as usize;
    let start=s.active_idx.saturating_sub(vis/3);
    let end=(start+vis+2).min(s.lines.len());
    for (i,line) in s.lines[start..end].iter().enumerate(){
        let abs=start+i; let y=i as f64*lh+lh;
        if abs==s.active_idx{
            cr.set_font_size(32.0); sca(cr,ACCENT,0.08);
            cr.rectangle(0.0,y-lh+8.0,w,lh-4.0); cr.fill().ok();
            sc(cr,ACCENT); cr.rectangle(0.0,y-lh+10.0,4.0,lh-8.0); cr.fill().ok();
            sc(cr,ACCENT); cr.move_to(20.0,y-12.0); cr.show_text(line).ok();
        } else if abs<s.active_idx{
            cr.set_font_size(20.0); sc(cr,MUTED); cr.move_to(20.0,y-12.0); cr.show_text(line).ok();
        } else {
            cr.set_font_size(22.0);
            sca(cr,TEXT,(1.0-(abs-s.active_idx) as f64*0.18).max(0.2));
            cr.move_to(20.0,y-12.0); cr.show_text(line).ok();
        }
    }
    let pct=s.active_idx as f64/s.lines.len() as f64;
    sca(cr,SURFACE,0.8); cr.rectangle(w-8.0,0.0,8.0,h); cr.fill().ok();
    sc(cr,ACCENT); cr.rectangle(w-8.0,0.0,8.0,h*pct); cr.fill().ok();
}

fn post(ep:&str,body:serde_json::Value)->Option<serde_json::Value>{
    let url=format!("{}{}",CONTENT_URL,ep);
    reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30))
        .build().ok()?.post(&url).json(&body).send().ok()?.json().ok()
}
fn get(ep:&str)->Option<serde_json::Value>{
    let url=format!("{}{}",CONTENT_URL,ep);
    reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(10))
        .build().ok()?.get(&url).send().ok()?.json().ok()
}

pub fn open_content_window(app:&gtk4::Application){
    let state=Arc::new(Mutex::new(ContentState::default()));
    let win=Window::builder().application(app).title("TitanDAW — Content Studio")
        .default_width(1100).default_height(700).build();
    let root=GBox::new(Orientation::Horizontal,0);
    let left=GBox::new(Orientation::Vertical,0); left.set_width_request(580);

    let tph=GBox::new(Orientation::Horizontal,6);
    tph.set_margin_start(8); tph.set_margin_top(6); tph.set_margin_bottom(6);
    let tpl=Label::new(Some("TELEPROMPTER")); tpl.set_halign(Align::Start); tpl.set_hexpand(true);
    let btn_prev=Button::with_label("◀"); let btn_next=Button::with_label("▶");
    let btn_reset=Button::with_label("↩"); let btn_auto=Button::with_label("AUTO");
    tph.append(&tpl); tph.append(&btn_prev); tph.append(&btn_next);
    tph.append(&btn_reset); tph.append(&btn_auto);

    let tp_canvas=DrawingArea::builder().vexpand(true).hexpand(true).build();
    { let s=state.clone();
      tp_canvas.set_draw_func(move|_,cr,w,h|{draw_teleprompter(cr,w as f64,h as f64,&s.lock().unwrap());}); }

    let tpc=GBox::new(Orientation::Horizontal,6);
    tpc.set_margin_start(8); tpc.set_margin_bottom(8);
    let status=Label::new(Some("Ready")); status.set_halign(Align::Start); status.set_hexpand(true);
    let btn_rec_start=Button::with_label("⏺ Record"); btn_rec_start.add_css_class("rec");
    let btn_rec_stop=Button::with_label("⏹ Stop");
    tpc.append(&btn_rec_start); tpc.append(&btn_rec_stop); tpc.append(&status);
    left.append(&tph); left.append(&tp_canvas); left.append(&tpc);

    let right=GBox::new(Orientation::Vertical,6); right.set_width_request(520);
    right.set_margin_start(8); right.set_margin_end(8); right.set_margin_top(8);

    let sr=GBox::new(Orientation::Horizontal,6);
    let styles=StringList::new(&["Hip-Hop","Trap","Drill","R&B","Spoken Word","Lo-fi","Gospel"]);
    let style_drop=DropDown::new(Some(styles),None::<gtk4::Expression>);
    style_drop.set_selected(0);
    let bars_adj=Adjustment::new(16.0,4.0,64.0,4.0,8.0,0.0);
    let bars_spin=SpinButton::new(Some(&bars_adj),1.0,0);
    sr.append(&Label::new(Some("Style:"))); sr.append(&style_drop);
    sr.append(&Label::new(Some("Bars:"))); sr.append(&bars_spin);

    let topic_lbl=Label::new(Some("Topic / Prompt:")); topic_lbl.set_halign(Align::Start);
    let topic_entry=Entry::builder().placeholder_text("e.g. grinding solo in Nashville").hexpand(true).build();
    let btn_gen=Button::with_label("Generate Lyrics →");
    let lyric_buf=TextBuffer::new(None::<&gtk4::TextTagTable>);
    let lyric_view=TextView::builder().buffer(&lyric_buf).editable(true)
        .wrap_mode(gtk4::WrapMode::Word).vexpand(true).build();
    let lyric_scroll=ScrolledWindow::builder().child(&lyric_view).vexpand(true).build();
    let btn_load_tp=Button::with_label("⬅ Load to Teleprompter");

    let export_row=GBox::new(Orientation::Horizontal,4);
    let video_entry=Entry::builder().placeholder_text("/path/to/video.mp4").hexpand(true).build();
    let btn_burn=Button::with_label("Burn SRT");
    let btn_export=Button::with_label("Export Cuts");
    export_row.append(&video_entry); export_row.append(&btn_burn); export_row.append(&btn_export);
    let export_status=Label::new(Some("")); export_status.set_halign(Align::Start);

    right.append(&sr); right.append(&topic_lbl); right.append(&topic_entry);
    right.append(&btn_gen); right.append(&lyric_scroll); right.append(&btn_load_tp);
    right.append(&Separator::new(Orientation::Horizontal));
    right.append(&export_row); right.append(&export_status);

    root.append(&left); root.append(&Separator::new(Orientation::Vertical)); root.append(&right);
    win.set_child(Some(&root));

    { let s=state.clone(); let c=tp_canvas.clone();
      btn_next.connect_clicked(move|_|{
          let mut st=s.lock().unwrap();
          if st.active_idx+1<st.lines.len(){st.active_idx+=1;} drop(st); c.queue_draw();
      }); }
    { let s=state.clone(); let c=tp_canvas.clone();
      btn_prev.connect_clicked(move|_|{
          let mut st=s.lock().unwrap();
          if st.active_idx>0{st.active_idx-=1;} drop(st); c.queue_draw();
      }); }
    { let s=state.clone(); let c=tp_canvas.clone();
      btn_reset.connect_clicked(move|_|{s.lock().unwrap().active_idx=0; c.queue_draw();}); }
    { let s=state.clone(); let c=tp_canvas.clone();
      btn_auto.connect_clicked(move|_|{
          let mut st=s.lock().unwrap(); st.auto_scroll=!st.auto_scroll; drop(st); c.queue_draw();
      }); }

    { let sl=status.clone();
      btn_rec_start.connect_clicked(move|_|{
          std::thread::spawn(||{post("/content/record/start",serde_json::json!({}));});
          sl.set_label("⏺ Recording...");
      }); }

    { let sl=status.clone(); let buf=lyric_buf.clone(); let s=state.clone(); let c=tp_canvas.clone();
      btn_rec_stop.connect_clicked(move|_|{
          sl.set_label("Processing...");
          let (tx,rx)=mpsc::channel::<(String,Vec<String>)>();
          std::thread::spawn(move||{
              post("/content/record/stop",serde_json::json!({}));
              std::thread::sleep(std::time::Duration::from_secs(4));
              if let Some(data)=get("/content/lyrics"){
                  let lyrics=data["lyrics"].as_str().unwrap_or("").to_string();
                  let lines:Vec<String>=data["lines"].as_array()
                      .map(|a|a.iter().filter_map(|v|v.as_str().map(|x|x.to_string())).collect())
                      .unwrap_or_default();
                  let _=tx.send((lyrics,lines));
              }
          });
          let b2=buf.clone(); let s2=s.clone(); let sl2=sl.clone(); let c2=c.clone();
          glib::timeout_add_local_once(Duration::from_secs(6),move||{
              if let Ok((lyrics,lines))=rx.try_recv(){
                  b2.set_text(&lyrics); s2.lock().unwrap().lines=lines;
                  sl2.set_label("✓ Transcribed"); c2.queue_draw();
              }
          });
      }); }

    { let te=topic_entry.clone(); let sd=style_drop.clone(); let bs=bars_spin.clone();
      let buf=lyric_buf.clone(); let s=state.clone(); let c=tp_canvas.clone(); let sl=status.clone();
      btn_gen.connect_clicked(move|_|{
          let topic=te.text().to_string();
          let idx=sd.selected() as usize;
          let styles_arr=["Hip-Hop","Trap","Drill","R&B","Spoken Word","Lo-fi","Gospel"];
          let style=styles_arr.get(idx).unwrap_or(&"Hip-Hop").to_string();
          let bars=bs.value() as u32;
          sl.set_label("Generating...");
          let (tx,rx)=mpsc::channel::<(String,Vec<String>)>();
          std::thread::spawn(move||{
              if let Some(data)=post("/content/generate",
                  serde_json::json!({"topic":topic,"style":style,"bars":bars})){
                  let lyrics=data["lyrics"].as_str().unwrap_or("").to_string();
                  let lines:Vec<String>=data["lines"].as_array()
                      .map(|a|a.iter().filter_map(|v|v.as_str().map(|x|x.to_string())).collect())
                      .unwrap_or_default();
                  let _=tx.send((lyrics,lines));
              }
          });
          let b2=buf.clone(); let s2=s.clone(); let sl2=sl.clone(); let c2=c.clone();
          glib::timeout_add_local_once(Duration::from_secs(31),move||{
              if let Ok((lyrics,lines))=rx.try_recv(){
                  b2.set_text(&lyrics); s2.lock().unwrap().lines=lines;
                  sl2.set_label("✓ Generated"); c2.queue_draw();
              }
          });
      }); }
    { let te=topic_entry.clone(); let b=btn_gen.clone();
      topic_entry.connect_activate(move|_|b.emit_clicked()); }

    { let buf=lyric_buf.clone(); let s=state.clone(); let c=tp_canvas.clone();
      btn_load_tp.connect_clicked(move|_|{
          let text=buf.text(&buf.start_iter(),&buf.end_iter(),false);
          let lines:Vec<String>=text.lines().map(|l|l.trim().to_string()).filter(|l|!l.is_empty()).collect();
          let lc=lines.clone();
          {let mut st=s.lock().unwrap(); st.lines=lines; st.active_idx=0;}
          std::thread::spawn(move||{post("/content/teleprompter/set",serde_json::json!({"lines":lc}));});
          c.queue_draw();
      }); }

    { let ve=video_entry.clone(); let sl=export_status.clone();
      btn_burn.connect_clicked(move|_|{
          let video=ve.text().to_string();
          let (tx,rx)=mpsc::channel::<String>();
          std::thread::spawn(move||{
              if let Some(d)=post("/content/burn",serde_json::json!({"video_path":video})){
                  let _=tx.send(d["output"].as_str().unwrap_or("").to_string());
              }
          });
          let sl2=sl.clone();
          glib::timeout_add_local_once(Duration::from_secs(60),move||{
              if let Ok(out)=rx.try_recv(){sl2.set_label(&format!("✓ Burned → {}",out));}
          });
      }); }

    { let ve=video_entry.clone(); let sl=export_status.clone();
      btn_export.connect_clicked(move|_|{
          let video=ve.text().to_string();
          let (tx,rx)=mpsc::channel::<String>();
          std::thread::spawn(move||{
              if let Some(d)=post("/content/export",serde_json::json!({"video_path":video})){
                  let _=tx.send(d["cuts"].to_string());
              }
          });
          let sl2=sl.clone();
          glib::timeout_add_local_once(Duration::from_secs(60),move||{
              if let Ok(cuts)=rx.try_recv(){sl2.set_label(&format!("✓ {}",&cuts[..cuts.len().min(80)]));}
          });
      }); }

    win.present();

    { let s=state.clone(); let c=tp_canvas.clone(); let mut ticks:u32=0;
      timeout_add_local(Duration::from_millis(16),move||{
          let mut st=s.lock().unwrap();
          if st.auto_scroll && !st.lines.is_empty(){
              ticks+=1;
              if ticks>=218{ticks=0;
                  if st.active_idx+1<st.lines.len(){st.active_idx+=1; c.queue_draw();}
              }
          } else {ticks=0;}
          glib::ControlFlow::Continue
      });
    }
}
