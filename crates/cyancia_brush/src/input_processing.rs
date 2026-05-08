use cyancia_math::curve::CubicBezierCurve;
use cyancia_shader_graph::graph::Graph;
use glam::Vec2;
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::render::{
    ComputedPenInput, MAX_SAMPLES_BETWEEN_INPUTS, PenInput,
    graph::{BrushGraphData, BrushGraphDataTuple},
};

#[derive(Debug, Clone, Copy, Default)]
pub struct RawPenInput {
    pub position: Vec2,
}

pub struct InputProcessor {
    samples: AllocRingBuffer<RawPenInput>,
    stabilizer: Box<dyn InputSampleStabilizer>,

    older_stable: Option<Vec2>,
    prev_stable: Option<Vec2>,

    last_sample: Option<ComputedPenInput>,
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
            last_sample: None,
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

    pub fn push(
        &mut self,
        input: RawPenInput,
        required_spacing: &Graph<BrushGraphData>,
    ) -> Vec<ComputedPenInput> {
        self.samples.enqueue(input);

        let Some(stablized) = self.stabilizer.stabilize(&self.samples) else {
            return Vec::new();
        };
        let new_pos = stablized.position;

        let pen_input = match (self.older_stable, self.prev_stable) {
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

        let Some(pen_input) = pen_input else {
            return Vec::new();
        };
        const BEZIER_SAMPLES: usize = 32;

        let p1 = pen_input.bezier_control_prev;
        let p2 = pen_input.bezier_control_cur;
        let p3 = pen_input.position;

        let p0 = match self.last_sample {
            Some(s) => s.position,
            None => {
                self.last_sample = Some(compute_pen_input(
                    &CubicBezierCurve::new(p3, p1, p2, p3),
                    1.0,
                ));
                return Vec::new();
            }
        };

        let curve = CubicBezierCurve::new(p0, p1, p2, p3);
        let new_computed = compute_pen_input(&curve, 1.0);

        // Build arc-length table.
        let mut total_arc = 0.0;
        let mut prev_p = p0;
        for i in 1..=BEZIER_SAMPLES {
            let p = curve.sample(i as f32 / BEZIER_SAMPLES as f32);
            total_arc += p.distance(prev_p);
            prev_p = p;
        }

        let spacing = compute_required_spacing(new_computed, required_spacing);
        if total_arc < 0.0001 || spacing <= 0.0 {
            return Vec::new();
        }
        let mut output = Vec::new();
        let mut last_sample = new_computed;
        let total_samples = (total_arc / spacing).floor() as u32;
        for p in 0..=total_samples {
            let t = p as f32 / total_samples as f32;
            let interpolated = compute_pen_input(&curve, t);

            output.push(interpolated);
            last_sample = interpolated;
        }
        self.last_sample = Some(last_sample);

        output
    }

    pub fn reset(&mut self) {
        self.samples.clear();
        self.older_stable = None;
        self.prev_stable = None;
        self.last_sample = None;
    }

    pub fn flush(
        &mut self,
        final_input: RawPenInput,
        required_spacing: &Graph<BrushGraphData>,
    ) -> Vec<ComputedPenInput> {
        let steps = self.stabilizer.convergence_steps();
        let mut result = Vec::new();
        for _ in 0..steps {
            result.extend(self.push(final_input, required_spacing));
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

fn compute_required_spacing(sample: ComputedPenInput, graph: &Graph<BrushGraphData>) -> f32 {
    let output = graph
        .run(&BrushGraphData { pen_input: sample }, Vec::new())
        .unwrap();

    // TODO Don't panic
    assert!(
        output.len() == 1,
        "Multiple outputs from required spacing graph not supported"
    );

    *output[0].as_ref::<f32>()
}

fn compute_pen_input(curve: &CubicBezierCurve, t: f32) -> ComputedPenInput {
    let position = curve.sample(t);
    let draw_direction_vec = curve.tangent(t);
    let draw_direction_angle = draw_direction_vec.y.atan2(draw_direction_vec.x);
    ComputedPenInput {
        position,
        draw_direction_vec,
        draw_direction_angle,
    }
}
