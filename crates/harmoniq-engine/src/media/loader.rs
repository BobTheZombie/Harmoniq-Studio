use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use crossbeam::channel::{unbounded, Receiver, Sender};
use symphonia::core::audio::{AudioBufferRef, Channels, SampleBuffer, Signal, SignalSpec};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSource;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::AudioClip;

#[derive(Debug, thiserror::Error)]
pub enum MediaLoaderError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Symphonia(#[from] SymphoniaError),
    #[error(transparent)]
    Hound(#[from] hound::Error),
    #[error("no supported audio tracks found in source")]
    NoSupportedTracks,
    #[error("media cache worker thread terminated")]
    CacheWorkerExited,
}

#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

impl DecodedAudio {
    pub fn to_clip(&self) -> AudioClip {
        AudioClip::with_sample_rate(self.sample_rate as f32, self.channels.clone())
    }
}

#[derive(Default, Debug)]
pub struct MediaLoader;

impl MediaLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn load_from_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<DecodedAudio, MediaLoaderError> {
        let file = File::open(path.as_ref())?;
        self.load_from_reader(file, Some(path.as_ref()))
    }

    pub fn load_from_reader<R>(
        &self,
        reader: R,
        hint_path: Option<&Path>,
    ) -> Result<DecodedAudio, MediaLoaderError>
    where
        R: MediaSource + 'static,
    {
        let mss = MediaSourceStream::new(Box::new(reader), Default::default());
        let mut hint = Hint::new();
        if let Some(path) = hint_path {
            if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
                hint.with_extension(ext);
            }
        }

        let probed = symphonia::default::get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;

        let mut format = probed.format;
        let (codec_params, track_id) = {
            let track = format
                .default_track()
                .ok_or(MediaLoaderError::NoSupportedTracks)?;
            (track.codec_params.clone(), track.id)
        };

        let mut decoder =
            symphonia::default::get_codecs().make(&codec_params, &DecoderOptions::default())?;

        let channel_layout = codec_params.channels.unwrap_or(Channels::FRONT_LEFT);
        let sample_rate = codec_params.sample_rate.unwrap_or(48_000);
        let spec = SignalSpec::new(sample_rate, channel_layout);

        let channel_count = spec.channels.count();
        let mut channel_data = vec![Vec::new(); channel_count];
        let mut sample_buffer: Option<SampleBuffer<f32>> = None;

        while let Ok(packet) = format.next_packet() {
            if packet.track_id() != track_id {
                continue;
            }

            let decoded = decoder.decode(&packet)?;
            match decoded {
                AudioBufferRef::F32(buffer) => {
                    for channel_index in 0..channel_count {
                        let channel = buffer.chan(channel_index);
                        channel_data[channel_index].extend_from_slice(channel);
                    }
                }
                other => {
                    let buf = sample_buffer.get_or_insert_with(|| {
                        SampleBuffer::<f32>::new(other.capacity() as u64, *other.spec())
                    });
                    buf.copy_interleaved_ref(other);
                    let samples = buf.samples();
                    let frames = samples.len() / channel_count;
                    for channel_index in 0..channel_count {
                        channel_data[channel_index].extend(
                            samples[channel_index..]
                                .iter()
                                .step_by(channel_count)
                                .take(frames)
                                .copied(),
                        );
                    }
                }
            }
        }

        Ok(DecodedAudio {
            sample_rate,
            channels: channel_data,
        })
    }

    pub fn write_wav<P: AsRef<Path>>(
        &self,
        audio: &DecodedAudio,
        path: P,
    ) -> Result<(), MediaLoaderError> {
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: audio.channels.len() as u16,
                sample_rate: audio.sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )?;

        if audio.channels.is_empty() {
            writer.finalize()?;
            return Ok(());
        }

        let frames = audio.channels[0].len();
        for frame in 0..frames {
            for channel in &audio.channels {
                let sample = channel.get(frame).copied().unwrap_or(0.0);
                writer.write_sample(sample)?;
            }
        }
        writer.finalize()?;
        Ok(())
    }
}

enum CacheCommand {
    Load {
        path: PathBuf,
        responder: Sender<Result<Arc<AudioClip>, MediaLoaderError>>,
    },
    Shutdown,
}

pub struct MediaCache {
    memory: Arc<Mutex<HashMap<PathBuf, Arc<AudioClip>>>>,
    commands: Sender<CacheCommand>,
    handle: Option<JoinHandle<()>>,
    disk_dir: PathBuf,
}

impl MediaCache {
    pub fn with_disk_cache<P: AsRef<Path>>(cache_dir: P) -> Result<Self, MediaLoaderError> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir)?;
        let (tx, rx) = unbounded();
        let memory = Arc::new(Mutex::new(HashMap::new()));
        let memory_clone = Arc::clone(&memory);
        let loader = MediaLoader::new();
        let disk_dir_clone = cache_dir.clone();
        let handle = thread::Builder::new()
            .name("media-cache".into())
            .spawn(move || cache_worker(loader, memory_clone, disk_dir_clone, rx))
            .map_err(MediaLoaderError::Io)?;

        Ok(Self {
            memory,
            commands: tx,
            handle: Some(handle),
            disk_dir: cache_dir,
        })
    }

    pub fn load<P: AsRef<Path>>(&self, path: P) -> Result<CacheHandle, MediaLoaderError> {
        let path = path.as_ref().to_path_buf();
        if let Some(clip) = self.memory.lock().unwrap().get(&path).cloned() {
            return Ok(CacheHandle::ready(clip));
        }
        let (tx, rx) = unbounded();
        self.commands
            .send(CacheCommand::Load {
                path,
                responder: tx,
            })
            .map_err(|_| MediaLoaderError::CacheWorkerExited)?;
        Ok(CacheHandle { receiver: Some(rx) })
    }
}

impl Drop for MediaCache {
    fn drop(&mut self) {
        let _ = self.commands.send(CacheCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn cache_worker(
    loader: MediaLoader,
    memory: Arc<Mutex<HashMap<PathBuf, Arc<AudioClip>>>>,
    disk_dir: PathBuf,
    receiver: Receiver<CacheCommand>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            CacheCommand::Shutdown => break,
            CacheCommand::Load { path, responder } => {
                let result = loader.load_from_path(&path).map(|decoded| {
                    let clip = decoded.to_clip();
                    let arc = Arc::new(clip);
                    memory
                        .lock()
                        .unwrap()
                        .insert(path.clone(), Arc::clone(&arc));
                    if let Err(err) = write_cache_file(&disk_dir, &path, &decoded) {
                        tracing::warn!("failed to persist media cache: {err}");
                    }
                    arc
                });
                let _ = responder.send(result);
            }
        }
    }
}

fn write_cache_file(
    cache_dir: &Path,
    source: &Path,
    decoded: &DecodedAudio,
) -> Result<(), MediaLoaderError> {
    let file_name = cache_file_name(source);
    let path = cache_dir.join(file_name);
    MediaLoader::new().write_wav(decoded, path)
}

fn cache_file_name(source: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    if let Ok(metadata) = source.metadata() {
        if let Ok(modified) = metadata.modified() {
            if let Ok(since) = modified.duration_since(std::time::SystemTime::UNIX_EPOCH) {
                since.as_nanos().hash(&mut hasher);
            }
        }
    }
    format!("{:016x}.wav", hasher.finish())
}

pub struct CacheHandle {
    receiver: Option<Receiver<Result<Arc<AudioClip>, MediaLoaderError>>>,
}

impl CacheHandle {
    fn ready(clip: Arc<AudioClip>) -> Self {
        let (tx, rx) = unbounded();
        let _ = tx.send(Ok(clip));
        Self { receiver: Some(rx) }
    }

    pub fn blocking_get(mut self) -> Result<Arc<AudioClip>, MediaLoaderError> {
        let rx = self
            .receiver
            .take()
            .ok_or(MediaLoaderError::CacheWorkerExited)?;
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(MediaLoaderError::CacheWorkerExited),
        }
    }
}
