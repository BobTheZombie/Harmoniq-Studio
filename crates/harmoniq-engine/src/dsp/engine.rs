use std::sync::Arc;

use arrayvec::ArrayVec;
use parking_lot::Mutex;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};

use harmoniq_dsp::buffer::{AudioBlock, AudioBlockMut};
use harmoniq_dsp::utils::flush_denormals;
#[cfg(feature = "no-denormals")]
use harmoniq_dsp::utils::NoDenormalsGuard;

use crate::buffers::{AudioView, AudioViewMut};
use crate::dsp::events::{MidiEvent, TransportClock};
use crate::dsp::graph::{DspGraph, GraphProcess};

#[cfg(feature = "openasio")]
use crate::backend::EngineRt;

const MIDI_RING_CAPACITY: usize = 512;
const MIDI_BUFFER_CAPACITY: usize = 512;

#[derive(Clone)]
pub struct MidiPort {
    inner: Arc<Mutex<HeapProducer<MidiEvent>>>,
}

impl MidiPort {
    #[inline]
    pub fn try_send(&self, event: MidiEvent) -> Result<(), MidiEvent> {
        let mut guard = self.inner.lock();
        guard.push(event)
    }

    #[inline]
    pub fn send(&self, event: MidiEvent) {
        let _ = self.try_send(event);
    }
}

pub struct RealtimeDspEngine {
    graph: DspGraph,
    midi_consumer: HeapConsumer<MidiEvent>,
    midi_producer: Arc<Mutex<HeapProducer<MidiEvent>>>,
    midi_buffer: ArrayVec<MidiEvent, MIDI_BUFFER_CAPACITY>,
    transport: TransportClock,
    sample_rate: f32,
    max_block: u32,
    in_ch: u32,
    out_ch: u32,
}

impl RealtimeDspEngine {
    pub fn new(graph: DspGraph) -> Self {
        Self::with_midi_capacity(graph, MIDI_RING_CAPACITY)
    }

    pub fn with_midi_capacity(graph: DspGraph, capacity: usize) -> Self {
        let ring = HeapRb::new(capacity.max(32));
        let (producer, consumer) = ring.split();
        Self {
            graph,
            midi_consumer: consumer,
            midi_producer: Arc::new(Mutex::new(producer)),
            midi_buffer: ArrayVec::new(),
            transport: TransportClock::new(),
            sample_rate: 44_100.0,
            max_block: 64,
            in_ch: 0,
            out_ch: 0,
        }
    }

    pub fn prepare(&mut self, sr: f32, max_block: u32, in_ch: u32, out_ch: u32) {
        self.sample_rate = sr;
        self.max_block = max_block.max(1);
        self.in_ch = in_ch;
        self.out_ch = out_ch;
        flush_denormals();
        self.graph
            .prepare(self.sample_rate, self.max_block, self.in_ch, self.out_ch);
        self.midi_buffer.clear();
    }

    pub fn graph_mut(&mut self) -> &mut DspGraph {
        &mut self.graph
    }

    pub fn midi_port(&self) -> MidiPort {
        MidiPort {
            inner: self.midi_producer.clone(),
        }
    }

    pub fn transport_clock(&self) -> TransportClock {
        self.transport.clone()
    }
}

#[cfg(feature = "openasio")]
impl EngineRt for RealtimeDspEngine {
    fn process(
        &mut self,
        inputs: AudioView<'_>,
        mut outputs: AudioViewMut<'_>,
        frames: u32,
    ) -> bool {
        let frames = frames.min(self.max_block);
        if frames == 0 {
            if let Some(buf) = outputs.interleaved_mut() {
                for sample in buf.iter_mut() {
                    *sample = 0.0;
                }
            } else if let Some(mut planar) = outputs.planar() {
                let planes = planar.planes();
                let frames = planar.frames();
                for plane in planes.iter_mut() {
                    if plane.is_null() {
                        continue;
                    }
                    for idx in 0..frames {
                        unsafe {
                            *plane.add(idx) = 0.0;
                        }
                    }
                }
            }
            return true;
        }

        #[cfg(feature = "no-denormals")]
        let _guard = NoDenormalsGuard::new();

        self.midi_buffer.clear();
        while let Some(event) = self.midi_consumer.pop() {
            let _ = self.midi_buffer.push(event);
        }

        let transport = self.transport.load();

        let input_block = if let Some(interleaved) = inputs.interleaved() {
            unsafe {
                AudioBlock::from_interleaved(interleaved.as_ptr(), inputs.channels() as u32, frames)
            }
        } else if let Some(planes) = inputs.planes_ptrs() {
            unsafe { AudioBlock::from_planar(planes, inputs.channels() as u32, frames) }
        } else {
            AudioBlock::empty()
        };

        let output_block = if let Some(interleaved) = outputs.interleaved_mut() {
            unsafe {
                AudioBlockMut::from_interleaved(
                    interleaved.as_mut_ptr(),
                    outputs.channels() as u32,
                    frames,
                )
            }
        } else if let Some(planes) = outputs.planes_ptrs_mut() {
            unsafe { AudioBlockMut::from_planar(planes, outputs.channels() as u32, frames) }
        } else {
            AudioBlockMut::empty()
        };

        self.graph.process(GraphProcess {
            inputs: input_block,
            outputs: output_block,
            frames,
            transport,
            midi: self.midi_buffer.as_slice(),
        });

        self.transport.advance_samples(frames);
        true
    }
}
