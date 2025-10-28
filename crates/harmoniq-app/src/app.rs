use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;

use crate::commands::Command;
use crate::state::session::{ChannelId, PatternId, Session};
use crate::ui::{
    channel_rack::{channel_rack_ui, ChannelRackProps},
    piano_roll::{piano_roll_ui, PianoRollState, PianoRollView},
};

pub struct HarmoniqApp {
    session: Session,
    selected_pattern: PatternId,
    command_tx: Sender<Command>,
    command_rx: Receiver<Command>,
    piano_roll: Option<PianoRollEditor>,
}

struct PianoRollEditor {
    channel_id: ChannelId,
    pattern_id: PatternId,
    state: PianoRollState,
}

impl HarmoniqApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (command_tx, command_rx) = unbounded();
        let mut session = Session::new_empty();
        if session.channels.is_empty() {
            session.add_instrument_channel("Channel 1".into(), None);
        }
        let selected_pattern = session
            .patterns
            .first()
            .map(|pattern| pattern.id)
            .unwrap_or(1);
        session.ensure_steps(1, selected_pattern);

        Self {
            session,
            selected_pattern,
            command_tx,
            command_rx,
            piano_roll: None,
        }
    }

    fn process_commands(&mut self) {
        while let Ok(command) = self.command_rx.try_recv() {
            self.handle_command(command);
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::AddPattern => {
                let id = self
                    .session
                    .add_pattern(format!("Pattern {}", self.session.patterns.len() + 1), 1);
                self.selected_pattern = id;
            }
            Command::SelectPattern(pattern_id) => {
                self.selected_pattern = pattern_id;
                if let Some(piano_roll) = &mut self.piano_roll {
                    piano_roll.pattern_id = pattern_id;
                }
            }
            Command::AddChannelInstrument { name, plugin_uid } => {
                let id = self.session.add_instrument_channel(name, plugin_uid);
                self.session.ensure_steps(id, self.selected_pattern);
            }
            Command::AddChannelSample { name, path } => {
                let id = self.session.add_sample_channel(name, path);
                self.session.ensure_steps(id, self.selected_pattern);
            }
            Command::RemoveChannel(channel_id) => {
                self.session.remove_channel(channel_id);
                if let Some(editor) = &self.piano_roll {
                    if editor.channel_id == channel_id {
                        self.piano_roll = None;
                    }
                }
            }
            Command::CloneChannel(channel_id) => {
                if let Some(new_id) = self.session.clone_channel(channel_id) {
                    self.session.ensure_steps(new_id, self.selected_pattern);
                }
            }
            Command::ToggleChannelMute(channel_id, value) => {
                if let Some(channel) = self.session.channel_mut(channel_id) {
                    channel.mute = value;
                }
            }
            Command::ToggleChannelSolo(channel_id, value) => {
                if let Some(channel) = self.session.channel_mut(channel_id) {
                    channel.solo = value;
                }
            }
            Command::ConvertStepsToMidi {
                channel_id,
                pattern_id,
            } => {
                self.convert_steps_to_midi(channel_id, pattern_id);
            }
            Command::OpenPianoRoll {
                channel_id,
                pattern_id,
            } => {
                self.piano_roll = Some(PianoRollEditor {
                    channel_id,
                    pattern_id,
                    state: PianoRollState::default(),
                });
            }
            Command::ClosePianoRoll => {
                self.piano_roll = None;
            }
            Command::PianoRollInsertNotes {
                channel_id,
                pattern_id,
                notes,
            } => {
                if let Some(clip) = self.session.clip_mut(channel_id, pattern_id) {
                    for (start, length, key, velocity) in notes {
                        clip.insert_note(start, length, key, velocity);
                    }
                }
            }
            Command::PianoRollDeleteNotes {
                channel_id,
                pattern_id,
                note_ids,
            } => {
                if let Some(clip) = self.session.clip_mut(channel_id, pattern_id) {
                    for note_id in note_ids {
                        clip.remove_note(note_id);
                    }
                }
            }
            Command::PianoRollSetNoteVelocity {
                channel_id,
                pattern_id,
                note_id,
                velocity,
            } => {
                if let Some(clip) = self.session.clip_mut(channel_id, pattern_id) {
                    if let Some(note) = clip.notes.get_mut(&note_id) {
                        note.velocity = velocity;
                    }
                }
            }
        }
    }

    fn convert_steps_to_midi(&mut self, channel_id: ChannelId, pattern_id: PatternId) {
        let ppq = {
            let session = &self.session;
            session
                .clip(channel_id, pattern_id)
                .map(|clip| clip.ppq)
                .unwrap_or(960)
        };
        let ticks_per_step = (ppq * 4) / 16;

        let triggered_steps: Vec<usize> = {
            let Some(channel) = self.session.channel_mut(channel_id) else {
                return;
            };
            let Some(steps) = channel.steps.get_mut(&pattern_id) else {
                return;
            };
            let mut indices = Vec::new();
            for (index, step) in steps.iter_mut().enumerate() {
                if *step {
                    indices.push(index);
                    *step = false;
                }
            }
            indices
        };

        let clip = match self.session.clip_mut(channel_id, pattern_id) {
            Some(clip) => clip,
            None => {
                let pattern = self.session.pattern_mut(pattern_id).unwrap();
                pattern.ensure_clip(channel_id)
            }
        };

        clip.notes.clear();
        clip.next_id = 1;

        for index in triggered_steps {
            let start_ticks = (index as u32) * ticks_per_step;
            clip.insert_note(start_ticks, ticks_per_step, 60, 100);
        }

        if let Some(editor) = &mut self.piano_roll {
            if editor.channel_id == channel_id {
                editor.state.selected_note = None;
            }
        }
    }
}

impl eframe::App for HarmoniqApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_commands();

        egui::SidePanel::left("channel_rack")
            .resizable(true)
            .show(ctx, |ui| {
                let props = ChannelRackProps {
                    session: &mut self.session,
                    selected_pattern: self.selected_pattern,
                    command_tx: self.command_tx.clone(),
                };
                channel_rack_ui(ui, props);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(editor) = &mut self.piano_roll {
                let view = PianoRollView {
                    session: &self.session,
                    channel_id: editor.channel_id,
                    pattern_id: editor.pattern_id,
                    state: &mut editor.state,
                    command_tx: self.command_tx.clone(),
                };
                piano_roll_ui(ui, view);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Select a channel and choose \"Edit in Piano Roll\" to begin.");
                });
            }
        });

        ctx.request_repaint();
    }
}
