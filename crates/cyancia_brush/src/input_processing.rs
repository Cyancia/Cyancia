use glam::Vec2;
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::render::PenInput;

#[derive(Debug, Clone, Copy, Default)]
pub struct RawPenInput {
    pub position: Vec2,
}

pub struct InputProcessor {
    samples: AllocRingBuffer<RawPenInput>,
    stabilizer: Box<dyn InputSampleStabilizer>,

    older_stable: Option<Vec2>,
    prev_stable: Option<Vec2>,
}

impl Default for InputProcessor {
    fn default() -> Self {
        Self::new(32, Box::new(BasicStabilizer))
    }
}

impl InputProcessor {
    pub fn new(buffer_size: usize, stabilizer: Box<dyn InputSampleStabilizer>) -> Self {
        Self {
            samples: AllocRingBuffer::new(buffer_size),
            stabilizer,
            older_stable: None,
            prev_stable: None,
        }
    }

    pub fn set_buffer_size(&mut self, buffered_samples: usize) {
        let mut new_buffer = AllocRingBuffer::new(buffered_samples);
        new_buffer.extend(self.samples.clone());
        self.samples = new_buffer;
    }

    pub fn set_stabilizer(&mut self, stabilizer: Box<dyn InputSampleStabilizer>) {
        self.stabilizer = stabilizer;
    }

    pub fn push(&mut self, input: RawPenInput) -> Option<PenInput> {
        self.samples.enqueue(input);

        let stabilized = self.stabilizer.stabilize(&self.samples)?;
        let new_pos = stabilized.position;

        let result = match (self.older_stable, self.prev_stable) {
            (Some(older), Some(prev)) => {
                let tangent_start = (new_pos - older) * 0.5;
                let tangent_end = new_pos - prev;
                let cp1 = prev + tangent_start / 3.0;
                let cp2 = new_pos - tangent_end / 3.0;
                Some(PenInput {
                    position: new_pos,
                    bezier_control_prev: cp1,
                    bezier_control_cur: cp2,
                })
            }
            (None, Some(prev)) => {
                let d = new_pos - prev;
                Some(PenInput {
                    position: new_pos,
                    bezier_control_prev: prev + d / 3.0,
                    bezier_control_cur: prev + d * (2.0 / 3.0),
                })
            }
            _ => None,
        };

        self.older_stable = self.prev_stable;
        self.prev_stable = Some(new_pos);
        result
    }

    pub fn reset(&mut self) {
        self.samples.clear();
        self.older_stable = None;
        self.prev_stable = None;
    }

    pub fn flush(&mut self, final_input: RawPenInput) -> Vec<PenInput> {
        let steps = self.stabilizer.convergence_steps();
        let mut result = Vec::new();
        for _ in 0..steps {
            if let Some(pen_input) = self.push(final_input) {
                result.push(pen_input);
            }
        }
        result
    }
}

pub trait InputSampleStabilizer: Send + Sync + 'static {
    fn stabilize(&mut self, inputs: &AllocRingBuffer<RawPenInput>) -> Option<RawPenInput>;

    fn convergence_steps(&self) -> usize;
}

pub struct GaussianStabilizer {
    kernel: Vec<f32>,
}

impl GaussianStabilizer {
    pub fn new(radius: usize) -> Self {
        if radius == 0 {
            return Self { kernel: vec![1.0] };
        }

        let window = 2 * radius + 1;
        let sigma = radius as f32 / 2.0;
        let center = radius as f32;

        let mut kernel: Vec<f32> = (0..window)
            .map(|i| {
                let x = i as f32 - center;
                (-x * x / (2.0 * sigma * sigma)).exp()
            })
            .collect();

        let sum: f32 = kernel.iter().sum();
        kernel.iter_mut().for_each(|k| *k /= sum);

        Self { kernel }
    }
}

impl InputSampleStabilizer for GaussianStabilizer {
    fn stabilize(&mut self, inputs: &AllocRingBuffer<RawPenInput>) -> Option<RawPenInput> {
        if inputs.is_empty() {
            return None;
        }

        let window = self.kernel.len();
        let available = inputs.len().min(window);

        let kernel_offset = window - available;
        let mut weighted_pos = Vec2::ZERO;
        let mut weight_sum = 0.0_f32;
        for (i, sample) in inputs.iter().rev().take(available).enumerate() {
            let w = self.kernel[kernel_offset + i];
            weighted_pos += sample.position * w;
            weight_sum += w;
        }

        Some(RawPenInput {
            position: weighted_pos / weight_sum,
        })
    }

    fn convergence_steps(&self) -> usize {
        self.kernel.len()
    }
}

pub struct BasicStabilizer;

impl InputSampleStabilizer for BasicStabilizer {
    fn stabilize(&mut self, inputs: &AllocRingBuffer<RawPenInput>) -> Option<RawPenInput> {
        inputs.back().copied()
    }

    fn convergence_steps(&self) -> usize {
        1
    }
}
