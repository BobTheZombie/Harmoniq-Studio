use std::f32::EPSILON;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use harmoniq_engine::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};
use harmoniq_plugin_sdk::{
    NativePlugin, ParameterDefinition, ParameterId, ParameterKind, ParameterLayout, ParameterSet,
    ParameterValue, PluginFactory,
};
use hound::{SampleFormat, WavSpec, WavWriter};

use crate::instruments::{load_sample_from_file, SampleBuffer};

const PARAM_OUTPUT_GAIN: &str = "edison.output_gain";
const PARAM_LOOP_SELECTION: &str = "edison.loop_selection";

fn audio_editor_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_OUTPUT_GAIN,
            "Output Gain",
            ParameterKind::continuous(0.0..=2.5, 1.0),
        )
        .with_unit("x")
        .with_description("Gain applied to playback during preview"),
        ParameterDefinition::new(
            PARAM_LOOP_SELECTION,
            "Loop Selection",
            ParameterKind::Toggle { default: false },
        )
        .with_description("When enabled the active selection continuously loops"),
    ])
}

#[derive(Debug, Clone)]
struct AudioClip {
    sample_rate: f32,
    channels: Vec<Vec<f32>>,
}

impl AudioClip {
    fn empty() -> Self {
        Self {
            sample_rate: 44_100.0,
            channels: vec![Vec::new()],
        }
    }

    fn from_sample_buffer(buffer: SampleBuffer) -> Self {
        let (sample_rate, mut channels) = buffer.into_channels();
        if channels.is_empty() {
            channels.push(Vec::new());
        }
        let max_len = channels.iter().map(|c| c.len()).max().unwrap_or(0);
        for channel in &mut channels {
            channel.resize(max_len, 0.0);
        }
        Self {
            sample_rate: sample_rate as f32,
            channels,
        }
    }

    #[cfg(test)]
    fn from_channels(sample_rate: f32, mut channels: Vec<Vec<f32>>) -> Self {
        if channels.is_empty() {
            channels.push(Vec::new());
        }
        let max_len = channels.iter().map(|c| c.len()).max().unwrap_or(0);
        for channel in &mut channels {
            channel.resize(max_len, 0.0);
        }
        Self {
            sample_rate,
            channels,
        }
    }

    fn len(&self) -> usize {
        self.channels.iter().map(|c| c.len()).max().unwrap_or(0)
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn channel_count(&self) -> usize {
        self.channels.len()
    }

    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    fn duration_seconds(&self) -> f32 {
        let sr = self.sample_rate.max(1.0);
        self.len() as f32 / sr
    }

    fn clamp_range(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let len = self.len();
        if len == 0 {
            return None;
        }
        let clamped_start = start.min(len.saturating_sub(1));
        let clamped_end = end.min(len);
        if clamped_end <= clamped_start {
            None
        } else {
            Some((clamped_start, clamped_end))
        }
    }

    fn sample(&self, channel: usize, index: usize) -> f32 {
        self.channels
            .get(channel)
            .and_then(|c| c.get(index))
            .copied()
            .unwrap_or(0.0)
    }

    fn sample_interpolated(&self, channel: usize, position: f32) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let index = position.floor() as usize;
        let frac = position - index as f32;
        let current = self.sample(channel, index);
        if frac <= 0.0 {
            return current;
        }
        let next = self.sample(channel, (index + 1).min(self.len().saturating_sub(1)));
        current + (next - current) * frac
    }

    fn trim_to_range(&mut self, start: usize, end: usize) {
        if let Some((start, end)) = self.clamp_range(start, end) {
            for channel in &mut self.channels {
                let mut trimmed = Vec::with_capacity(end - start);
                trimmed.extend_from_slice(&channel[start..end]);
                *channel = trimmed;
            }
        } else {
            let channel_count = self.channel_count().max(1);
            self.channels = vec![Vec::new(); channel_count];
        }
    }

    fn reverse_range(&mut self, start: usize, end: usize) {
        if let Some((start, end)) = self.clamp_range(start, end) {
            for channel in &mut self.channels {
                channel[start..end].reverse();
            }
        }
    }

    fn peak_in_range(&self, start: usize, end: usize) -> f32 {
        if let Some((start, end)) = self.clamp_range(start, end) {
            let mut peak = 0.0;
            for channel in &self.channels {
                let channel_end = end.min(channel.len());
                for sample in &channel[start..channel_end] {
                    let abs = sample.abs();
                    if abs > peak {
                        peak = abs;
                    }
                }
            }
            peak
        } else {
            0.0
        }
    }

    fn normalize_range(&mut self, start: usize, end: usize) {
        let peak = self.peak_in_range(start, end);
        if peak <= EPSILON {
            return;
        }
        let gain = 1.0 / peak;
        if let Some((start, end)) = self.clamp_range(start, end) {
            for channel in &mut self.channels {
                let channel_end = end.min(channel.len());
                for sample in &mut channel[start..channel_end] {
                    *sample = (*sample * gain).clamp(-1.0, 1.0);
                }
            }
        }
    }

    fn fade_in_range(&mut self, start: usize, end: usize) {
        if let Some((start, end)) = self.clamp_range(start, end) {
            let length = end.saturating_sub(start);
            if length <= 1 {
                return;
            }
            let denom = (length - 1) as f32;
            for channel in &mut self.channels {
                let channel_end = end.min(channel.len());
                for (i, sample) in channel[start..channel_end].iter_mut().enumerate() {
                    let factor = i as f32 / denom;
                    *sample *= factor;
                }
            }
        }
    }

    fn fade_out_range(&mut self, start: usize, end: usize) {
        if let Some((start, end)) = self.clamp_range(start, end) {
            let length = end.saturating_sub(start);
            if length <= 1 {
                return;
            }
            let denom = (length - 1) as f32;
            for channel in &mut self.channels {
                let channel_end = end.min(channel.len());
                for (i, sample) in channel[start..channel_end].iter_mut().enumerate() {
                    let factor = (length - 1 - i) as f32 / denom;
                    *sample *= factor;
                }
            }
        }
    }

    fn overview(&self, segments: usize) -> Vec<(f32, f32)> {
        let len = self.len();
        if len == 0 || segments == 0 {
            return Vec::new();
        }
        let frames_per_segment = ((len as f32 / segments as f32).ceil() as usize).max(1);
        let mut overview = Vec::with_capacity(segments);
        for segment in 0..segments {
            let start = segment * frames_per_segment;
            if start >= len {
                break;
            }
            let end = ((segment + 1) * frames_per_segment).min(len);
            let mut min_value = f32::MAX;
            let mut max_value = f32::MIN;
            for channel in &self.channels {
                if start >= channel.len() {
                    continue;
                }
                let channel_end = end.min(channel.len());
                for sample in &channel[start..channel_end] {
                    min_value = min_value.min(*sample);
                    max_value = max_value.max(*sample);
                }
            }
            if min_value == f32::MAX {
                min_value = 0.0;
            }
            if max_value == f32::MIN {
                max_value = 0.0;
            }
            overview.push((min_value, max_value));
        }
        overview
    }

    fn write_to_wav(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        if self.is_empty() {
            return Err(anyhow!("no audio data available"));
        }
        let path_ref = path.as_ref();
        let channels = self.channel_count().max(1);
        let sample_rate = if self.sample_rate <= 0.0 {
            44_100
        } else {
            self.sample_rate.round() as u32
        };
        let spec = WavSpec {
            channels: channels as u16,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let mut writer = WavWriter::create(path_ref, spec)
            .with_context(|| format!("failed to create {:?}", path_ref))?;
        let frames = self.len();
        for index in 0..frames {
            for channel in 0..channels {
                let sample = self.sample(channel, index);
                writer.write_sample(sample)?;
            }
        }
        writer.finalize()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioClipMetrics {
    pub sample_rate: f32,
    pub channels: usize,
    pub length_samples: usize,
    pub length_seconds: f32,
    pub peak: f32,
}

#[derive(Debug, Clone)]
pub struct AudioEditorPlugin {
    engine_sample_rate: f32,
    clip: AudioClip,
    selection: Option<(usize, usize)>,
    playing: bool,
    play_start: usize,
    play_end: usize,
    play_position: f32,
    loop_enabled: bool,
    source_path: Option<PathBuf>,
    parameters: ParameterSet,
}

impl Default for AudioEditorPlugin {
    fn default() -> Self {
        let parameters = ParameterSet::new(audio_editor_layout());
        Self {
            engine_sample_rate: 44_100.0,
            clip: AudioClip::empty(),
            selection: None,
            playing: false,
            play_start: 0,
            play_end: 0,
            play_position: 0.0,
            loop_enabled: false,
            source_path: None,
            parameters,
        }
    }
}

impl AudioEditorPlugin {
    pub fn set_engine_sample_rate(&mut self, sample_rate: f32) {
        self.engine_sample_rate = sample_rate;
    }

    pub fn has_clip(&self) -> bool {
        !self.clip.is_empty()
    }

    pub fn load_audio(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path_ref = path.as_ref();
        let buffer = load_sample_from_file(path_ref)
            .with_context(|| format!("failed to load {:?}", path_ref))?;
        self.clip = AudioClip::from_sample_buffer(buffer);
        self.selection = None;
        self.playing = false;
        self.play_start = 0;
        self.play_end = self.clip.len();
        self.play_position = 0.0;
        self.source_path = Some(path_ref.to_path_buf());
        Ok(())
    }

    pub fn clear_clip(&mut self) {
        self.clip = AudioClip::empty();
        self.selection = None;
        self.playing = false;
        self.play_start = 0;
        self.play_end = 0;
        self.play_position = 0.0;
        self.source_path = None;
    }

    pub fn export_wav(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        self.clip.write_to_wav(path)
    }

    pub fn clip_metrics(&self) -> Option<AudioClipMetrics> {
        if self.clip.is_empty() {
            return None;
        }
        let length_samples = self.clip.len();
        let length_seconds = self.clip.duration_seconds();
        let peak = self.clip.peak_in_range(0, length_samples);
        Some(AudioClipMetrics {
            sample_rate: self.clip.sample_rate(),
            channels: self.clip.channel_count(),
            length_samples,
            length_seconds,
            peak,
        })
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    pub fn waveform_overview(&self, segments: usize) -> Vec<(f32, f32)> {
        self.clip.overview(segments)
    }

    pub fn clip_length_samples(&self) -> usize {
        self.clip.len()
    }

    pub fn clip_duration_seconds(&self) -> f32 {
        self.clip.duration_seconds()
    }

    pub fn clip_sample_rate(&self) -> f32 {
        self.clip.sample_rate()
    }

    pub fn clip_channel_count(&self) -> usize {
        self.clip.channel_count()
    }

    pub fn selection_samples(&self) -> Option<(usize, usize)> {
        self.selection
    }

    pub fn selection_seconds(&self) -> Option<(f32, f32)> {
        self.selection.map(|(start, end)| {
            let sr = self.clip.sample_rate().max(1.0);
            (start as f32 / sr, end as f32 / sr)
        })
    }

    pub fn set_selection_seconds(&mut self, start: f32, end: f32) {
        let sr = self.clip.sample_rate().max(1.0);
        let start_samples = (start.max(0.0) * sr).floor() as usize;
        let end_samples = (end.max(0.0) * sr).ceil() as usize;
        self.set_selection_samples(start_samples, end_samples);
    }

    pub fn set_selection_fraction(&mut self, start: f32, end: f32) {
        let length = self.clip.len();
        if length == 0 {
            self.selection = None;
            return;
        }
        let start = (start.clamp(0.0, 1.0) * length as f32).floor() as usize;
        let end = (end.clamp(0.0, 1.0) * length as f32).ceil() as usize;
        self.set_selection_samples(start, end);
    }

    pub fn set_selection_samples(&mut self, start: usize, end: usize) {
        if let Some((start, end)) = self.clip.clamp_range(start, end) {
            self.selection = Some((start, end));
        } else {
            self.selection = None;
        }
        self.reconcile_state();
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.reconcile_state();
    }

    pub fn playhead_seconds(&self) -> f32 {
        if self.clip.is_empty() {
            0.0
        } else {
            self.play_position / self.clip.sample_rate().max(1.0)
        }
    }

    pub fn playhead_fraction(&self) -> f32 {
        let len = self.clip.len();
        if len == 0 {
            0.0
        } else {
            (self.play_position / len as f32).clamp(0.0, 1.0)
        }
    }

    pub fn set_playhead_fraction(&mut self, fraction: f32) {
        let len = self.clip.len();
        if len == 0 {
            self.play_position = 0.0;
        } else {
            let clamped = fraction.clamp(0.0, 1.0);
            let position = clamped * len as f32;
            self.play_position = position.clamp(self.play_start as f32, self.play_end as f32);
        }
    }

    pub fn set_playhead_seconds(&mut self, seconds: f32) {
        let sr = self.clip.sample_rate().max(1.0);
        let position = (seconds.max(0.0) * sr) as f32;
        let len = self.clip.len() as f32;
        self.play_position = position.clamp(0.0, len);
    }

    pub fn start_playback(&mut self, selection_only: bool) {
        let (start, end) = self.playback_bounds(selection_only);
        if end <= start {
            self.playing = false;
            return;
        }
        self.play_start = start;
        self.play_end = end;
        self.play_position = start as f32;
        self.playing = true;
    }

    pub fn stop_playback(&mut self) {
        self.playing = false;
        self.play_position = self.play_start as f32;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn loop_enabled(&self) -> bool {
        self.loop_enabled
    }

    pub fn set_loop_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let id = ParameterId::from(PARAM_LOOP_SELECTION);
        self.set_parameter(&id, ParameterValue::Toggle(enabled))
            .map_err(|err| err.to_string())?;
        self.loop_enabled = enabled;
        Ok(())
    }

    pub fn output_gain(&self) -> f32 {
        self.parameters
            .get(&ParameterId::from(PARAM_OUTPUT_GAIN))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.0)
    }

    pub fn set_output_gain(&mut self, gain: f32) -> Result<(), String> {
        let id = ParameterId::from(PARAM_OUTPUT_GAIN);
        self.set_parameter(&id, ParameterValue::Continuous(gain))
            .map_err(|err| err.to_string())
    }

    pub fn apply_trim(&mut self) -> Result<(), String> {
        let (start, end) = self
            .selection
            .filter(|(start, end)| end > start)
            .ok_or_else(|| "Select a region before trimming".to_string())?;
        self.clip.trim_to_range(start, end);
        self.selection = None;
        self.play_start = 0;
        self.play_end = self.clip.len();
        self.play_position = 0.0;
        self.playing = false;
        Ok(())
    }

    pub fn apply_reverse(&mut self) {
        let (start, end) = self.selection.unwrap_or((0, self.clip.len()));
        self.clip.reverse_range(start, end.max(start + 1));
    }

    pub fn apply_normalize(&mut self) {
        let (start, end) = self.selection.unwrap_or((0, self.clip.len()));
        self.clip.normalize_range(start, end.max(start + 1));
    }

    pub fn apply_fade_in(&mut self) {
        let (start, end) = self.selection.unwrap_or((0, self.clip.len()));
        self.clip.fade_in_range(start, end.max(start + 1));
    }

    pub fn apply_fade_out(&mut self) {
        let (start, end) = self.selection.unwrap_or((0, self.clip.len()));
        self.clip.fade_out_range(start, end.max(start + 1));
    }

    fn playback_bounds(&self, selection_only: bool) -> (usize, usize) {
        let len = self.clip.len();
        if len == 0 {
            return (0, 0);
        }
        if selection_only {
            if let Some((start, end)) = self.selection {
                if end > start {
                    return (start, end.min(len));
                }
            }
            (0, len)
        } else {
            if let Some((start, end)) = self.selection {
                if end > start {
                    return (start, end.min(len));
                }
            }
            (0, len)
        }
    }

    fn reconcile_state(&mut self) {
        let len = self.clip.len();
        if len == 0 {
            self.selection = None;
            self.play_start = 0;
            self.play_end = 0;
            self.play_position = 0.0;
            self.playing = false;
            return;
        }
        if let Some((start, end)) = self.selection {
            if end <= start || start >= len {
                self.selection = None;
            } else {
                let clamped_end = end.min(len);
                self.selection = Some((start.min(len - 1), clamped_end));
            }
        }
        let (start, end) = self.playback_bounds(false);
        self.play_start = start;
        self.play_end = end;
        if self.play_position < self.play_start as f32 {
            self.play_position = self.play_start as f32;
        } else if self.play_position > self.play_end as f32 {
            self.play_position = self.play_end as f32;
            self.playing = false;
        }
    }

    fn playback_step(&self) -> f32 {
        let clip_rate = if self.clip.sample_rate() <= 0.0 {
            self.engine_sample_rate.max(1.0)
        } else {
            self.clip.sample_rate()
        };
        let engine_rate = self.engine_sample_rate.max(1.0);
        clip_rate / engine_rate
    }
}

impl AudioProcessor for AudioEditorPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.editor.edison",
            "Edison Audio Editor",
            "Harmoniq Labs",
        )
        .with_description(
            "Standalone sample editor for detailed waveform capture and transformation",
        )
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.engine_sample_rate = config.sample_rate;
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let frames = buffer.len();
        let channel_count = buffer.as_slice().len();
        let mut output = vec![vec![0.0; frames]; channel_count];

        if self.playing && !self.clip.is_empty() && self.play_end > self.play_start {
            let step = self.playback_step();
            let gain = self.output_gain();
            let mut position = self.play_position;
            let start = self.play_start as f32;
            let end = self.play_end as f32;
            let clip_channels = self.clip.channel_count().max(1);
            let mut playing = true;

            for sample_index in 0..frames {
                if position >= end {
                    if self.loop_enabled && end > start {
                        position = start;
                    } else {
                        playing = false;
                        break;
                    }
                }

                for channel in 0..channel_count {
                    let source_channel = if channel < clip_channels {
                        channel
                    } else {
                        channel % clip_channels
                    };
                    let value = self.clip.sample_interpolated(source_channel, position) * gain;
                    output[channel][sample_index] = value;
                }

                position += step;
            }

            self.playing = playing;
            if playing {
                self.play_position = position;
            } else {
                self.play_position = end;
            }
        }

        for (channel_buffer, channel_data) in buffer.channels_mut().zip(output.into_iter()) {
            channel_buffer.copy_from_slice(&channel_data);
        }

        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for AudioEditorPlugin {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    fn on_parameter_changed(
        &mut self,
        id: &ParameterId,
        value: &ParameterValue,
    ) -> Result<(), harmoniq_plugin_sdk::PluginParameterError> {
        if id.as_str() == PARAM_LOOP_SELECTION {
            self.loop_enabled = value.as_toggle().unwrap_or(false);
        }
        Ok(())
    }
}

pub struct AudioEditorPluginFactory;

impl PluginFactory for AudioEditorPluginFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.editor.edison",
            "Edison Audio Editor",
            "Harmoniq Labs",
        )
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(audio_editor_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(AudioEditorPlugin::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_and_reverse_ranges() {
        let mut clip = AudioClip::from_channels(48_000.0, vec![vec![0.0, 0.1, 0.2, 0.3, 0.4]]);
        clip.trim_to_range(1, 4);
        assert_eq!(clip.len(), 3);
        assert!((clip.sample(0, 0) - 0.1).abs() < 1e-6);
        clip.reverse_range(0, 3);
        assert!((clip.sample(0, 0) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn fade_and_normalize() {
        let mut clip = AudioClip::from_channels(44_100.0, vec![vec![1.0, 1.0, 1.0, 1.0]]);
        clip.fade_in_range(0, 4);
        assert!((clip.sample(0, 0)).abs() < 1e-6);
        clip.fade_out_range(0, 4);
        assert!((clip.sample(0, 3)).abs() < 1e-6);
        clip.normalize_range(0, 4);
        assert!((clip.peak_in_range(0, 4) - 1.0).abs() < 1e-6);
    }
}
