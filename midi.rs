// midi.rs — MIDI input (AudioBox USB 96 + LoopMIDI)   JCH-2026-011
use midir::{MidiInput, MidiInputConnection};
use crossbeam_channel::{unbounded, Sender, Receiver};
use log::{info, warn};

#[derive(Debug, Clone)]
pub enum MidiEvent {
    NoteOn  { channel: u8, pitch: u8, velocity: u8 },
    NoteOff { channel: u8, pitch: u8 },
    CC      { channel: u8, cc: u8, value: u8 },
    PitchBend { channel: u8, value: i16 },
}

pub struct MidiRouter {
    _conn:  Option<MidiInputConnection<()>>,
    pub rx: Receiver<MidiEvent>,
}

impl MidiRouter {
    pub fn new(port_name_hint: &str) -> Self {
        let (tx, rx): (Sender<MidiEvent>, Receiver<MidiEvent>) = unbounded();

        let midi_in = MidiInput::new("TitanDAW").expect("MIDI init failed");
        let ports   = midi_in.ports();

        info!("MIDI ports:");
        let mut target = None;
        for (i, p) in ports.iter().enumerate() {
            let name = midi_in.port_name(p).unwrap_or_default();
            info!("  [{i}] {name}");
            if name.to_lowercase().contains(&port_name_hint.to_lowercase()) {
                target = Some(i);
            }
        }

        let conn = if let Some(idx) = target {
            let port = &ports[idx];
            let name = midi_in.port_name(port).unwrap_or_default();
            info!("MIDI connected: {name}");

            let conn = midi_in.connect(port, "titan-midi-in", move |_stamp, msg, _| {
                if msg.len() < 2 { return; }
                let status  = msg[0];
                let kind    = status & 0xF0;
                let channel = status & 0x0F;

                let event = match kind {
                    0x90 if msg.len() >= 3 && msg[2] > 0 => Some(MidiEvent::NoteOn {
                        channel, pitch: msg[1], velocity: msg[2]
                    }),
                    0x80 | 0x90 if msg.len() >= 3 => Some(MidiEvent::NoteOff {
                        channel, pitch: msg[1]
                    }),
                    0xB0 if msg.len() >= 3 => Some(MidiEvent::CC {
                        channel, cc: msg[1], value: msg[2]
                    }),
                    0xE0 if msg.len() >= 3 => {
                        let val = ((msg[2] as i16) << 7 | msg[1] as i16) - 8192;
                        Some(MidiEvent::PitchBend { channel, value: val })
                    },
                    _ => None,
                };

                if let Some(ev) = event {
                    let _ = tx.send(ev);
                }
            }, ()).ok();
            conn
        } else {
            warn!("MIDI port '{}' not found — MIDI disabled", port_name_hint);
            None
        };

        Self { _conn: conn, rx }
    }

    pub fn list_ports() -> Vec<String> {
        let midi_in = MidiInput::new("TitanDAW-scan").unwrap();
        midi_in.ports().iter().map(|p| {
            midi_in.port_name(p).unwrap_or_default()
        }).collect()
    }
}
