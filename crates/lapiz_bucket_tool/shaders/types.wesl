struct BucketParams {
    seed: vec2u,
    fill_color: vec4f,
    threshold: f32,
    alpha_threshold: f32,
    close_gap: u32,
    grow: i32,
    feather: u32,
    image_size: vec2u,
    transparent_mode: u32,
}

const INVALID_LABEL: u32 = 0xFFFFFFFFu;
