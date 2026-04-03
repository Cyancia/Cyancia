import package::brush::brush_types::{PenInput, ComputedPenInput, PenInputSampler, OutputSamples, StrokeInfo, ESTIMATION_WORKGROUP_SIZE};
import package::render::math::{unsigned_div_ceil};

@group(0) @binding(0) var<storage> new_input: PenInput;
@group(0) @binding(1) var<storage, read_write> input_sampler: PenInputSampler;
@group(0) @binding(2) var<storage, read_write> output_samples: OutputSamples;
@group(0) @binding(3) var<storage, read_write> estimate_dispatch: vec3u;
@group(0) @binding(4) var<storage, read_write> stroke_info: StrokeInfo;

fn compute_spacing_factor(src: ComputedPenInput, dst: ComputedPenInput) -> f32 {
    //CODEGENFLAG_COMPUTED_GRAPH_SPACING_FACTOR
    return distance(src.position, dst.position); // TODO
}

fn compute_required_spacing(sample: ComputedPenInput) -> f32 {
    //CODEGENFLAG_COMPUTED_GRAPH_REQUIRED_SPACING
    return 1.0; // TODO
}

@compute
// TODO: Parallel
@workgroup_size(1, 1, 1)
fn main() {
    output_samples.n_samples = 0;

    let new_sample = ComputedPenInput(
        new_input.position,
    );

    if input_sampler.has_last_sample == 0 {
        input_sampler.last_input = new_input;
        input_sampler.last_sample = new_sample;
        input_sampler.has_last_sample = 1;

        output_samples.samples[output_samples.n_samples] = new_sample;
        output_samples.n_samples += 1;
        return;
    }

    let spacing = compute_required_spacing(new_sample);
    let total_spacing = compute_spacing_factor(new_sample, input_sampler.last_sample);
    var remaining_spacing = total_spacing;
    while output_samples.n_samples < arrayLength(&output_samples.samples) {
        let t = (total_spacing - remaining_spacing) / total_spacing;
        let interpolated_sample = ComputedPenInput(
            mix(input_sampler.last_sample.position, new_sample.position, t),
        );

        output_samples.samples[output_samples.n_samples] = interpolated_sample;
        output_samples.n_samples += 1;
        remaining_spacing -= spacing;

        if remaining_spacing < spacing {
            input_sampler.last_input = new_input;
            input_sampler.last_sample = interpolated_sample;
            break;
        }
    }

    estimate_dispatch = vec3u(1, 1, unsigned_div_ceil(output_samples.n_samples, ESTIMATION_WORKGROUP_SIZE.z));
    stroke_info.total_dabs += output_samples.n_samples;
}
