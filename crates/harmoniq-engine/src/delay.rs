use crate::buffer::AudioBuffer;

pub(crate) struct DelayCompensator {
    buffers: Vec<Vec<f32>>,
    write_positions: Vec<usize>,
    delay_samples: usize,
    capacity: usize,
    block_size: usize,
}

impl DelayCompensator {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            write_positions: Vec::new(),
            delay_samples: 0,
            capacity: 0,
            block_size: 0,
        }
    }

    pub fn configure(&mut self, channels: usize, delay_samples: usize, block_size: usize) {
        let block_size = block_size.max(1);
        let capacity = delay_samples + block_size;

        if self.buffers.len() != channels {
            self.buffers = vec![vec![0.0; capacity]; channels];
            self.write_positions = vec![0; channels];
        } else if self.capacity != capacity {
            for buffer in &mut self.buffers {
                buffer.resize(capacity, 0.0);
            }
            for position in &mut self.write_positions {
                *position = 0;
            }
        }

        if self.delay_samples != delay_samples || self.block_size != block_size {
            for buffer in &mut self.buffers {
                buffer.fill(0.0);
            }
            for position in &mut self.write_positions {
                *position = 0;
            }
        }

        self.delay_samples = delay_samples;
        self.capacity = capacity;
        self.block_size = block_size;
    }

    pub fn process(&mut self, buffer: &mut AudioBuffer) {
        if self.delay_samples == 0 || self.capacity == 0 {
            return;
        }

        let capacity = self.capacity;
        let delay = self.delay_samples.min(capacity - 1);

        for (channel_index, channel) in buffer.channels_mut().enumerate() {
            if channel_index >= self.buffers.len() {
                break;
            }
            let storage = &mut self.buffers[channel_index];
            if storage.len() != capacity {
                continue;
            }

            let mut write_pos = self.write_positions[channel_index] % capacity;
            let mut read_pos = if write_pos >= delay {
                write_pos - delay
            } else {
                write_pos + capacity - delay
            };

            for sample in channel.iter_mut() {
                let delayed = storage[read_pos];
                storage[write_pos] = *sample;
                *sample = delayed;

                write_pos += 1;
                if write_pos == capacity {
                    write_pos = 0;
                }

                read_pos += 1;
                if read_pos == capacity {
                    read_pos = 0;
                }
            }
            self.write_positions[channel_index] = write_pos;
        }
    }

    pub fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        for position in &mut self.write_positions {
            *position = 0;
        }
        self.delay_samples = 0;
    }

    pub fn delay_samples(&self) -> usize {
        self.delay_samples
    }
}
