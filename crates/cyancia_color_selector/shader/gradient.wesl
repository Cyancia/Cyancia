import color::{
    gray::{Gray, gray_into_xyz},
    lab::{Lab, lab_into_xyz},
    lch::{Lch, lch_into_xyz},
    okhsl::{OkHsl, okhsl_into_xyz},
    okhsv::{OkHsv, okhsv_into_xyz},
    oklab::{OkLab, oklab_into_xyz},
    oklch::{OkLch, oklch_into_xyz},
    rgb::{Rgb, rgb_from_xyz, rgb_into_xyz},
    xyz::Xyz,
};

struct VertexInput {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

struct Settings {
    rgb_to_xyz: mat3x3f,
    xyz_to_rgb: mat3x3f,
    channel_ranges: array<vec4f, 3>,
    reference: vec3f,
    // Gray,
    // Lab,
    // Lch,
    // OkHsl,
    // OkHsv,
    // OkLab,
    // OkLch,
    // Rgb,
    // Xyz,
    color_model: u32,
    variable_channels: u32,
}

@group(0) @binding(0) var<uniform> settings: Settings;

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    return VertexOutput(vec4f(in.position, 0.0, 1.0), in.uv);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4f {
    var ch = settings.reference;
    var j = 0u;
    for (var i = 0u; i < 3u; i += 1u) {
        if ((settings.variable_channels >> i) & 1u) != 0u {
            ch[i] = mix(settings.channel_ranges[i].x, settings.channel_ranges[i].y, in.uv[j]);
            j += 1u;
        }
    }

    var xyz = Xyz(0.0, 0.0, 0.0);
    switch settings.color_model {
        case 0: {
            xyz = gray_into_xyz(Gray(ch[0]));
        }
        case 1: {
            xyz = lab_into_xyz(Lab(ch[0], ch[1], ch[2]));
        }
        case 2: {
            xyz = lch_into_xyz(Lch(ch[0], ch[1], ch[2]));
        }
        case 3: {
            xyz = okhsl_into_xyz(OkHsl(ch[0], ch[1], ch[2]));
        }
        case 4: {
            xyz = okhsv_into_xyz(OkHsv(ch[0], ch[1], ch[2]));
        }
        case 5: {
            xyz = oklab_into_xyz(OkLab(ch[0], ch[1], ch[2]));
        }
        case 6: {
            xyz = oklch_into_xyz(OkLch(ch[0], ch[1], ch[2]));
        }
        case 7: {
            xyz = rgb_into_xyz(Rgb(ch[0], ch[1], ch[2]), settings.rgb_to_xyz);
        }
        case 8: {
            xyz = Xyz(ch[0], ch[1], ch[2]);
        }
        default: {}
    }

    let rgb = rgb_from_xyz(xyz, settings.xyz_to_rgb);
    return vec4f(rgb.r, rgb.g, rgb.b, 1.0);
}
