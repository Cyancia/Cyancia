import package::brush_types::{PenInput, ComputedPenInput, PenInputSampler, OutputSamples, StrokeInfo, Time, CanvasResources};
import render::math::{unsigned_div_ceil, bezier_eval, bezier_tangent_eval, mix_angle};

@group(0) @binding(0) var<storage> new_input: PenInput;
@group(0) @binding(1) var<storage, read_write> input_sampler: PenInputSampler;
@group(0) @binding(2) var<storage, read_write> output_samples: OutputSamples;
@group(0) @binding(3) var<storage, read_write> bounds_eval_dispatch: vec3u;
@group(0) @binding(4) var<storage> canvas_resources: CanvasResources;
@group(0) @binding(5) var<storage, read_write> initial_pen_input: ComputedPenInput;

//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

const BEZIER_SAMPLES: u32 = 32u;

fn compute_required_spacing(graph_input: ComputedPenInput) -> f32 {
    //CODEGENFLAG_COMPUTED_GRAPH_REQUIRED_SPACING
}

fn bezier_t_for_arc_length(table: ptr<function, array<f32, BEZIER_SAMPLES + 1u>>, arc_goal: f32) -> f32 {
    const BISECT_ITERATIONS: u32 = u32(ceil(log2(f32(BEZIER_SAMPLES)))) + 1u;

    var lo: u32 = 0u;
    var hi: u32 = BEZIER_SAMPLES;
    for (var i = 0u; i < BISECT_ITERATIONS; i++) {
        let mid = (lo + hi) / 2u;
        if (*table)[mid] < arc_goal {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lo_arc = (*table)[lo];
    let hi_arc = (*table)[hi];
    let lo_t = f32(lo) / f32(BEZIER_SAMPLES);
    let hi_t = f32(hi) / f32(BEZIER_SAMPLES);
    if hi_arc <= lo_arc {
        return lo_t;
    }
    return mix(lo_t, hi_t, (arc_goal - lo_arc) / (hi_arc - lo_arc));
}

fn compute_pen_input(last: PenInput, current: PenInput, p0: vec2f, p1: vec2f, p2: vec2f, p3: vec2f, t: f32, dab_index: u32) -> ComputedPenInput {
    let position = bezier_eval(p0, p1, p2, p3, t);
    let pressure = mix(last.pressure, current.pressure, t);
    let tilt = vec2f(
        mix_angle(last.tilt.x, current.tilt.x, t),
        mix_angle(last.tilt.y, current.tilt.y, t),
    );
    let angle = vec2f(
        mix_angle(last.angle.x, current.angle.x, t),
        mix_angle(last.angle.y, current.angle.y, t),
    );
    let time = Time(mix(last.time.now, current.time.now, t), last.time.stroke_begin);
    let draw_direction_vec = bezier_tangent_eval(p0, p1, p2, p3, t);
    let draw_direction_angle = atan2(draw_direction_vec.y, draw_direction_vec.x);
    return ComputedPenInput(position, draw_direction_vec, tilt, angle, draw_direction_angle, pressure, dab_index, time);
}

@compute
// TODO: Parallel
@workgroup_size(1, 1, 1)
fn main() {
    output_samples.n_samples = 0;

    let p0 = input_sampler.last_sample.position;
    let p1 = new_input.bezier_control_prev;
    let p2 = new_input.bezier_control_cur;
    let p3 = new_input.position;

    let new_computed = compute_pen_input(new_input, new_input, p0, p1, p2, p3, 1.0, 0);

    if input_sampler.has_last_sample == 0 {
        input_sampler.last_input = new_input;
        input_sampler.last_sample = new_computed;
        input_sampler.has_last_sample = 1;

        bounds_eval_dispatch = vec3u(0u);
        return;
    }

    var arc_table: array<f32, BEZIER_SAMPLES + 1u>;
    arc_table[0] = 0.0;
    var prev_p = p0;
    for (var i: u32 = 1u; i <= BEZIER_SAMPLES; i++) {
        let t = f32(i) / f32(BEZIER_SAMPLES);
        let p = bezier_eval(p0, p1, p2, p3, t);
        arc_table[i] = arc_table[i - 1u] + distance(prev_p, p);
        prev_p = p;
    }
    let total_arc = arc_table[BEZIER_SAMPLES];

    let spacing = compute_required_spacing(new_computed);
    if total_arc < 0.0001 || spacing <= 0.0 {
        input_sampler.last_input = new_input;
        return;
    }

    var remaining_arc = total_arc;
    while output_samples.n_samples < arrayLength(&output_samples.samples) {
        let target_arc = total_arc - remaining_arc;
        let t = bezier_t_for_arc_length(&arc_table, target_arc);
        let interpolated = compute_pen_input(input_sampler.last_input, new_input, p0, p1, p2, p3, t, input_sampler.next_dab_index);
        input_sampler.next_dab_index += 1u;

        output_samples.samples[output_samples.n_samples] = interpolated;
        output_samples.n_samples += 1u;
        remaining_arc -= spacing;

        if input_sampler.has_initial_input == 0 {
            initial_pen_input = interpolated;
            input_sampler.has_initial_input = 1;
        }

        if remaining_arc < spacing {
            break;
        }
    }

    input_sampler.last_input = new_input;
    input_sampler.last_sample = output_samples.samples[output_samples.n_samples - 1u];

    bounds_eval_dispatch = vec3u(1u, 1u, unsigned_div_ceil(output_samples.n_samples, 16u));
}
