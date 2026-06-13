struct BucketParams {
    seed: vec2u,
    fill_color: vec4f,
    threshold: f32,
    alpha_threshold: f32,
    grow: i32,
}

const INVALID_LABEL: u32 = 0xFFFFFFFFu;
