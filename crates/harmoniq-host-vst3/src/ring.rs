use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Context, Result};
use memmap2::{MmapMut, MmapOptions};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[repr(C)]
struct RingHeader {
    write_index: AtomicU32,
    read_index: AtomicU32,
    frames: u32,
    channels: u32,
}

/// Describes a shared audio ring buffer that can be opened from another process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SharedAudioRingDescriptor {
    pub path: PathBuf,
    pub frames: u32,
    pub channels: u32,
}

/// Shared-memory audio ring with zero-copy semantics. The ring exposes a single block of
/// interleaved audio samples that can be swapped between producer and consumer without copying.
#[derive(Debug)]
pub struct SharedAudioRing {
    descriptor: SharedAudioRingDescriptor,
    _file: NamedTempFile,
    mmap: MmapMut,
}

impl SharedAudioRing {
    pub fn create(frames: u32, channels: u32) -> Result<Self> {
        let mut file = tempfile::Builder::new()
            .prefix("harmoniq-audio-ring")
            .tempfile()
            .context("failed to allocate shared audio ring backing file")?;

        let total_samples = frames as usize * channels as usize;
        let data_len = total_samples * std::mem::size_of::<f32>();
        let header_len = std::mem::size_of::<RingHeader>();
        let total_len = header_len + data_len;

        file.as_file_mut()
            .set_len(total_len as u64)
            .context("failed to size shared audio ring")?;

        let mut mmap = unsafe { MmapOptions::new().len(total_len).map_mut(file.as_file())? };
        unsafe {
            let header_ptr = mmap.as_mut_ptr() as *mut RingHeader;
            std::ptr::write(
                header_ptr,
                RingHeader {
                    write_index: AtomicU32::new(0),
                    read_index: AtomicU32::new(0),
                    frames,
                    channels,
                },
            );
        }

        let descriptor = SharedAudioRingDescriptor {
            path: file.path().to_path_buf(),
            frames,
            channels,
        };

        Ok(Self {
            descriptor,
            mmap,
            _file: file,
        })
    }

    pub fn descriptor(&self) -> &SharedAudioRingDescriptor {
        &self.descriptor
    }

    fn header(&self) -> &RingHeader {
        unsafe { &*(self.mmap.as_ptr() as *const RingHeader) }
    }

    fn data_ptr(&self) -> *const f32 {
        unsafe { self.mmap.as_ptr().add(std::mem::size_of::<RingHeader>()) as *const f32 }
    }

    fn data_slice_mut(&mut self) -> &mut [f32] {
        let total_samples = self.descriptor.frames as usize * self.descriptor.channels as usize;
        unsafe {
            std::slice::from_raw_parts_mut(
                self.mmap
                    .as_mut_ptr()
                    .add(std::mem::size_of::<RingHeader>()) as *mut f32,
                total_samples,
            )
        }
    }

    /// Write a full interleaved audio block into the ring.
    pub fn write_block(&mut self, samples: &[f32]) -> Result<()> {
        let expected = self.descriptor.frames as usize * self.descriptor.channels as usize;
        anyhow::ensure!(
            samples.len() == expected,
            "audio block must be frames * channels"
        );

        self.data_slice_mut().copy_from_slice(samples);
        self.header().write_index.fetch_add(1, Ordering::Release);
        Ok(())
    }

    /// Read the most recent interleaved audio block without copying.
    pub fn latest_block(&self) -> (&[f32], u32) {
        let header = self.header();
        let generation = header.write_index.load(Ordering::Acquire);
        let data = unsafe {
            std::slice::from_raw_parts(
                self.data_ptr(),
                self.descriptor.frames as usize * self.descriptor.channels as usize,
            )
        };
        (data, generation)
    }

    /// Mark that the reader has consumed the latest audio block.
    pub fn acknowledge(&self, generation: u32) {
        self.header()
            .read_index
            .store(generation, Ordering::Release);
    }
}

impl SharedAudioRingDescriptor {
    /// Open the shared audio ring from an existing descriptor.
    pub fn open(&self) -> Result<(File, MmapMut)> {
        let file = File::options().read(true).write(true).open(&self.path)?;
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        Ok((file, mmap))
    }

    pub fn read_latest_block(&self) -> Result<(Vec<f32>, u32)> {
        let (_file, mmap) = self.open()?;
        let header = unsafe { &*(mmap.as_ptr() as *const RingHeader) };
        let generation = header.write_index.load(Ordering::Acquire);
        let total_samples = header.frames as usize * header.channels as usize;
        let data_ptr =
            unsafe { mmap.as_ptr().add(std::mem::size_of::<RingHeader>()) as *const f32 };
        let slice = unsafe { std::slice::from_raw_parts(data_ptr, total_samples) };
        Ok((slice.to_vec(), generation))
    }

    pub fn write_block(&self, samples: &[f32]) -> Result<()> {
        let (_file, mut mmap) = self.open()?;
        let header = unsafe { &*(mmap.as_ptr() as *const RingHeader) };
        let total_samples = header.frames as usize * header.channels as usize;
        anyhow::ensure!(samples.len() == total_samples, "audio block size mismatch");
        let data_ptr =
            unsafe { mmap.as_mut_ptr().add(std::mem::size_of::<RingHeader>()) as *mut f32 };
        let slice = unsafe { std::slice::from_raw_parts_mut(data_ptr, total_samples) };
        slice.copy_from_slice(samples);
        header.write_index.fetch_add(1, Ordering::Release);
        Ok(())
    }
}
