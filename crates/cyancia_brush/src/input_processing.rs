use cyancia_math::curve::CubicBezierCurve;
use cyancia_shader_graph::graph::Graph;
use glam::Vec2;
use gpui::App;
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::render::{ComputedPenInput, Time, graph::BrushGraphData};

#[derive(Debug, Clone, Copy, Default)]
pub struct RawPenInput {
    pub position: Vec2,
    pub time: Time,
}

pub struct InputProcessor {
    samples: AllocRingBuffer<RawPenInput>,
    stabilizer: Box<dyn InputSampleStabilizer>,
    older_stable: Option<RawPenInput>,
    prev_stable: Option<RawPenInput>,
    last_sample: Option<ComputedPenInput>,
    arc_offset: f32,
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
            arc_offset: 0.0,
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
        cx: &App,
    ) -> Vec<ComputedPenInput> {
        self.samples.enqueue(input);

        let Some(stabilized) = self.stabilizer.stabilize(&self.samples) else {
            return Vec::new();
        };

        let curve = match (self.older_stable, self.prev_stable) {
            (Some(older), Some(prev)) => {
                let new_pos = stabilized.position;
                let tangent_start = (new_pos - older.position) * 0.5;
                let tangent_end = new_pos - prev.position;
                let cp1 = prev.position + tangent_start / 3.0;
                let cp2 = new_pos - tangent_end / 3.0;
                Some(CubicBezierCurve::new(prev.position, cp1, cp2, new_pos))
            }
            (None, Some(prev)) => {
                let new_pos = stabilized.position;
                let d = new_pos - prev.position;
                let cp1 = prev.position + d / 3.0;
                let cp2 = prev.position + d * (2.0 / 3.0);
                Some(CubicBezierCurve::new(prev.position, cp1, cp2, new_pos))
            }
            _ => None,
        };

        self.older_stable = self.prev_stable;
        self.prev_stable = Some(stabilized);

        let Some(curve) = curve else {
            return Vec::new();
        };

        let from = self.older_stable.unwrap();
        let to = self.prev_stable.unwrap();

        const ARC_SAMPLES: usize = 64;
        let mut arc_table: Vec<(f32, f32)> = Vec::with_capacity(ARC_SAMPLES + 1);
        arc_table.push((0.0, 0.0));
        let mut prev_p = curve.sample(0.0);
        let mut total_arc = 0.0_f32;
        for i in 1..=ARC_SAMPLES {
            let t = i as f32 / ARC_SAMPLES as f32;
            let p = curve.sample(t);
            total_arc += p.distance(prev_p);
            arc_table.push((total_arc, t));
            prev_p = p;
        }

        let mid_sample = compute_pen_input(&curve, 0.5, &from, &to);
        let spacing = compute_required_spacing(mid_sample, required_spacing, cx);

        if total_arc < 0.0001 || spacing <= 0.0 {
            return Vec::new();
        }

        let mut output = Vec::new();
        let mut stamp_arc = self.arc_offset;

        while stamp_arc <= total_arc {
            let t = arc_length_to_t(&arc_table, stamp_arc);
            let sample = compute_pen_input(&curve, t, &from, &to);
            output.push(sample);
            self.last_sample = Some(sample);
            stamp_arc += spacing;
        }

        self.arc_offset = stamp_arc - total_arc;

        output
    }

    pub fn reset(&mut self) {
        self.samples.clear();
        self.older_stable = None;
        self.prev_stable = None;
        self.last_sample = None;
        self.arc_offset = 0.0;
    }

    pub fn flush(
        &mut self,
        final_input: RawPenInput,
        required_spacing: &Graph<BrushGraphData>,
        cx: &App,
    ) -> Vec<ComputedPenInput> {
        let steps = self.stabilizer.convergence_steps();
        let mut result = Vec::new();
        for _ in 0..steps {
            result.extend(self.push(final_input, required_spacing, cx));
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
            time: inputs.back().unwrap().time,
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

fn arc_length_to_t(arc_table: &[(f32, f32)], arc: f32) -> f32 {
    if arc <= 0.0 {
        return arc_table[0].1;
    }
    let last = arc_table.last().unwrap();
    if arc >= last.0 {
        return last.1;
    }
    let i = arc_table.partition_point(|&(a, _)| a < arc);
    let (a0, t0) = arc_table[i - 1];
    let (a1, t1) = arc_table[i];
    if (a1 - a0).abs() < f32::EPSILON {
        return t0;
    }
    t0 + (t1 - t0) * (arc - a0) / (a1 - a0)
}

fn compute_required_spacing(
    sample: ComputedPenInput,
    graph: &Graph<BrushGraphData>,
    cx: &App,
) -> f32 {
    let output = graph
        .run(&BrushGraphData { pen_input: sample }, Vec::new(), cx)
        .unwrap();

    assert!(
        output.len() == 1,
        "Multiple outputs from required spacing graph not supported"
    );

    *output[0].as_ref::<f32>()
}

fn compute_pen_input(
    curve: &CubicBezierCurve,
    t: f32,
    from: &RawPenInput,
    to: &RawPenInput,
) -> ComputedPenInput {
    let position = curve.sample(t);
    let draw_direction_vec = curve.tangent(t);
    let draw_direction_angle = draw_direction_vec.y.atan2(draw_direction_vec.x);
    ComputedPenInput {
        position,
        draw_direction_vec,
        draw_direction_angle,
        time: Time {
            now: from.time.now + t * (to.time.now - from.time.now),
            stroke_begin: from.time.stroke_begin,
        },
    }
}
