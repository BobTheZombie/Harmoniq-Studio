use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;

use crate::commands::Command;
use crate::state::session::{ChannelId, MidiNote, PatternId, Session};
use crate::ui::{
    channel_rack::{self, ChannelRackState},
    piano_roll::{self, PianoRollContext, PianoRollState},
};

pub struct HarmoniqApp {
    session: Session,
    commands_tx: Sender<Command>,
    commands_rx: Receiver<Command>,
    channel_rack_state: ChannelRackState,
    piano_roll_state: PianoRollState,
    open_piano_roll: Option<(ChannelId, PatternId)>,
}

impl HarmoniqApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let session = Session::new_empty();
        let (tx, rx) = unbounded();
        Self {
            session,
            commands_tx: tx,
            commands_rx: rx,
            channel_rack_state: ChannelRackState::default(),
            piano_roll_state: PianoRollState::default(),
            open_piano_roll: None,
        }
    }

    fn process_commands(&mut self, ctx: &egui::Context) {
        let mut should_repaint = false;
        while let Ok(command) = self.commands_rx.try_recv() {
            self.handle_command(command);
            should_repaint = true;
        }
        if should_repaint {
            ctx.request_repaint();
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::AddPattern => {
                let id = self.session.add_pattern();
                self.session.current_pattern = id;
            }
            Command::SelectPattern(pattern_id) => {
                if self.session.pattern_exists(pattern_id) {
                    self.session.current_pattern = pattern_id;
                }
            }
            Command::AddChannelInstrument { name, plugin_uid } => {
                let id = self
                    .session
                    .add_instrument_channel(name, Some(plugin_uid.clone()));
                for pattern in self
                    .session
                    .patterns
                    .iter()
                    .map(|p| p.id)
                    .collect::<Vec<_>>()
                {
                    self.session.ensure_steps(pattern, id);
                    let _ = self.session.clip_mut(pattern, id);
                }
            }
            Command::AddChannelSample { name, path } => {
                let id = self.session.add_sample_channel(name);
                if let Some(channel) = self
                    .session
                    .channels
                    .iter_mut()
                    .find(|channel| channel.id == id)
                {
                    channel.target_plugin_uid = Some(path.to_string_lossy().to_string());
                }
                for pattern in self
                    .session
                    .patterns
                    .iter()
                    .map(|p| p.id)
                    .collect::<Vec<_>>()
                {
                    self.session.ensure_steps(pattern, id);
                    let _ = self.session.clip_mut(pattern, id);
                }
            }
            Command::RemoveChannel(channel_id) => {
                self.session.remove_channel(channel_id);
                if self.open_piano_roll.is_some_and(|(ch, _)| ch == channel_id) {
                    self.open_piano_roll = None;
                }
            }
            Command::CloneChannel(channel_id) => {
                if let Some(new_id) = self.session.clone_channel(channel_id) {
                    for pattern in self
                        .session
                        .patterns
                        .iter()
                        .map(|p| p.id)
                        .collect::<Vec<_>>()
                    {
                        self.session.ensure_steps(pattern, new_id);
                    }
                }
            }
            Command::ToggleChannelMute(channel_id, mute) => {
                if let Some(channel) = self
                    .session
                    .channels
                    .iter_mut()
                    .find(|channel| channel.id == channel_id)
                {
                    channel.mute = mute;
                }
            }
            Command::ToggleChannelSolo(channel_id, solo) => {
                if let Some(channel) = self
                    .session
                    .channels
                    .iter_mut()
                    .find(|channel| channel.id == channel_id)
                {
                    channel.solo = solo;
                }
            }
            Command::ConvertStepsToMidi {
                channel_id,
                pattern_id,
            } => {
                if self.session.pattern_exists(pattern_id)
                    && self.session.channel_exists(channel_id)
                {
                    let steps = {
                        let lane = self.session.ensure_steps(pattern_id, channel_id);
                        lane.clone()
                    };
                    let clip = self.session.clip_mut(pattern_id, channel_id);
                    clip.clear();
                    let ticks_per_step = (clip.ppq / 4).max(1);
                    for (index, active) in steps.into_iter().enumerate() {
                        if active {
                            let start = index as u32 * ticks_per_step;
                            let note = MidiNote {
                                id: 0,
                                start_ticks: start,
                                length_ticks: ticks_per_step,
                                key: 60,
                                velocity: 100,
                            };
                            clip.insert(note);
                        }
                    }
                }
            }
            Command::OpenPianoRoll {
                channel_id,
                pattern_id,
            } => {
                if self.session.channel_exists(channel_id)
                    && self.session.pattern_exists(pattern_id)
                {
                    self.open_piano_roll = Some((channel_id, pattern_id));
                }
            }
            Command::ClosePianoRoll => {
                self.open_piano_roll = None;
            }
            Command::PianoRollInsertNotes {
                channel_id,
                pattern_id,
                notes,
            } => {
                if self.session.channel_exists(channel_id)
                    && self.session.pattern_exists(pattern_id)
                {
                    let clip = self.session.clip_mut(pattern_id, channel_id);
                    let midi_notes =
                        notes
                            .into_iter()
                            .map(|(start, length, key, velocity)| MidiNote {
                                id: 0,
                                start_ticks: start,
                                length_ticks: length,
                                key,
                                velocity,
                            });
                    clip.insert_many(midi_notes);
                }
            }
            Command::PianoRollDeleteNotes {
                channel_id,
                pattern_id,
                note_ids,
            } => {
                if self.session.channel_exists(channel_id)
                    && self.session.pattern_exists(pattern_id)
                {
                    let clip = self.session.clip_mut(pattern_id, channel_id);
                    clip.remove_many(&note_ids);
                }
            }
            Command::PianoRollSetNoteVelocity {
                channel_id,
                pattern_id,
                note_id,
                velocity,
            } => {
                if self.session.channel_exists(channel_id)
                    && self.session.pattern_exists(pattern_id)
                {
                    let clip = self.session.clip_mut(pattern_id, channel_id);
                    clip.set_note_velocity(note_id, velocity);
                }
            }
        }
    }
}

impl eframe::App for HarmoniqApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_commands(ctx);

        egui::SidePanel::left("channel_rack")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                channel_rack::render(
                    ui,
                    &mut self.session,
                    &mut self.channel_rack_state,
                    &self.commands_tx,
                );
            });

        self.process_commands(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some((channel_id, pattern_id)) = self.open_piano_roll {
                let channel_name = self
                    .session
                    .channels
                    .iter()
                    .find(|channel| channel.id == channel_id)
                    .map(|channel| channel.name.clone());
                let pattern_name = self
                    .session
                    .patterns
                    .iter()
                    .find(|pattern| pattern.id == pattern_id)
                    .map(|pattern| pattern.name.clone());
                if let (Some(channel_name), Some(pattern_name)) = (channel_name, pattern_name) {
                    let (ppq, notes_snapshot) = {
                        let clip = self.session.clip_mut(pattern_id, channel_id);
                        let snapshot = clip
                            .notes
                            .iter()
                            .map(|(id, note)| (*id, note.clone()))
                            .collect::<Vec<_>>();
                        (clip.ppq, snapshot)
                    };
                    let context = PianoRollContext {
                        channel_id,
                        pattern_id,
                        channel_name: &channel_name,
                        pattern_name: &pattern_name,
                        ppq,
                        notes: &notes_snapshot,
                    };
                    piano_roll::render(ui, &mut self.piano_roll_state, &context, &self.commands_tx);
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Channel or pattern not found");
                    });
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Select “Edit in Piano Roll” from a channel to start editing notes.");
                });
            }
        });

        self.process_commands(ctx);

        if let Some((channel_id, pattern_id)) = self.open_piano_roll {
            if !self.session.channel_exists(channel_id) || !self.session.pattern_exists(pattern_id)
            {
                self.open_piano_roll = None;
            }
        }
    }

    fn on_close_event(&mut self) -> bool {
        true
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {}
}
