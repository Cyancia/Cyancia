import package::{
    xyz::Xyz,
    gray::{Gray, gray_into_xyz, gray_from_xyz},
    hsl::{hsl_into_rgb, hsl_from_rgb},
    hsv::{hsv_into_rgb, hsv_from_rgb},
    lab::{Lab, lab_into_xyz, lab_from_xyz},
    lch::{Lch, lch_into_xyz, lch_from_xyz},
    oklab::{OkLab, oklab_into_xyz, oklab_from_xyz},
    oklch::{OkLch, oklch_into_xyz, oklch_from_xyz},
    okhsl::{OkHsl, okhsl_into_xyz, okhsl_from_xyz},
    okhsv::{OkHsv, okhsv_into_xyz, okhsv_from_xyz},
    rgb::{Rgb, rgb_into_xyz, rgb_from_xyz},
};

import wgsl_test::expectWithin;

fn expect_xyz_near(lhs: Xyz, rhs: Xyz) {
    // Use absolute tolerance only, to match rust behavior.
    expectWithin(lhs.x, rhs.x, 0.0, 1e-4);
    expectWithin(lhs.y, rhs.y, 0.0, 1e-4);
    expectWithin(lhs.z, rhs.z, 0.0, 1e-4);
}

fn expect_rgb_near(lhs: Rgb, rhs: Rgb) {
    expectWithin(lhs.r, rhs.r, 0.0, 1e-4);
    expectWithin(lhs.g, rhs.g, 0.0, 1e-4);
    expectWithin(lhs.b, rhs.b, 0.0, 1e-4);
}

const SRGB_LINEAR_TO_XYZ: mat3x3f = mat3x3f(
    vec3f(0.4124564, 0.2126729, 0.0193339),
    vec3f(0.3575761, 0.7151522, 0.1191920),
    vec3f(0.1804375, 0.0721750, 0.9503041),
);

const XYZ_TO_SRGB_LINEAR: mat3x3f = mat3x3f(
    vec3f(3.2404542, -0.9692660, 0.0556434),
    vec3f(-1.5371385, 1.8760108, -0.2040259),
    vec3f(-0.4985314, 0.0415560, 1.0572252),
);

const TEST_SEGMENTS: u32 = 16;

@test
fn roundtrip_test_gray() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        let v = f32(a) / f32(TEST_SEGMENTS - 1u);
        let color = Gray(v);
        let xyz = gray_into_xyz(color);
        let roundtrip = gray_from_xyz(xyz);
        let roundtrip_xyz = gray_into_xyz(roundtrip);
        expect_xyz_near(xyz, roundtrip_xyz);
    }
}

@test
fn roundtrip_test_hsl() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let rgb = Rgb(
                    f32(a) / f32(TEST_SEGMENTS - 1u),
                    f32(b) / f32(TEST_SEGMENTS - 1u),
                    f32(c) / f32(TEST_SEGMENTS - 1u),
                );
                let roundtrip = hsl_into_rgb(hsl_from_rgb(rgb));
                expect_rgb_near(rgb, roundtrip);
            }
        }
    }
}

@test
fn roundtrip_test_hsv() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let rgb = Rgb(
                    f32(a) / f32(TEST_SEGMENTS - 1u),
                    f32(b) / f32(TEST_SEGMENTS - 1u),
                    f32(c) / f32(TEST_SEGMENTS - 1u),
                );
                let roundtrip = hsv_into_rgb(hsv_from_rgb(rgb));
                expect_rgb_near(rgb, roundtrip);
            }
        }
    }
}

@test
fn roundtrip_test_lab() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let l = f32(a) / f32(TEST_SEGMENTS - 1u) * 100.0;
                let a2 = f32(b) / f32(TEST_SEGMENTS - 1u) * 60.0 - 30.0;
                let b2 = f32(c) / f32(TEST_SEGMENTS - 1u) * 60.0 - 30.0;
                let color = Lab(l, a2, b2);
                let xyz = lab_into_xyz(color);
                let roundtrip = lab_from_xyz(xyz);
                let roundtrip_xyz = lab_into_xyz(roundtrip);
                expect_xyz_near(xyz, roundtrip_xyz);
            }
        }
    }
}

@test
fn roundtrip_test_lch() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let l = f32(a) / f32(TEST_SEGMENTS - 1u) * 100.0;
                let chroma = f32(b) / f32(TEST_SEGMENTS - 1u) * 10.0;
                let hue = f32(c) / f32(TEST_SEGMENTS - 1u) * 360.0;
                let color = Lch(l, chroma, hue);
                let xyz = lch_into_xyz(color);
                let roundtrip = lch_from_xyz(xyz);
                let roundtrip_xyz = lch_into_xyz(roundtrip);
                expect_xyz_near(xyz, roundtrip_xyz);
            }
        }
    }
}

@test
fn roundtrip_test_oklab() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let l = f32(a) / f32(TEST_SEGMENTS - 1u);
                let a2 = f32(b) / f32(TEST_SEGMENTS - 1u) - 0.5;
                let b2 = f32(c) / f32(TEST_SEGMENTS - 1u) - 0.5;
                let color = OkLab(l, a2, b2);
                let xyz = oklab_into_xyz(color);
                let roundtrip = oklab_from_xyz(xyz);
                let roundtrip_xyz = oklab_into_xyz(roundtrip);
                expect_xyz_near(xyz, roundtrip_xyz);
            }
        }
    }
}

@test
fn roundtrip_test_oklch() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let l = f32(a) / f32(TEST_SEGMENTS - 1u);
                let chroma = f32(b) / f32(TEST_SEGMENTS - 1u);
                let hue = f32(c) / f32(TEST_SEGMENTS - 1u) * 360.0;
                let color = OkLch(l, chroma, hue);
                let xyz = oklch_into_xyz(color);
                let roundtrip = oklch_from_xyz(xyz);
                let roundtrip_xyz = oklch_into_xyz(roundtrip);
                expect_xyz_near(xyz, roundtrip_xyz);
            }
        }
    }
}

@test
fn roundtrip_test_okhsl() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let hue = f32(a) / f32(TEST_SEGMENTS - 1u) * 360.0;
                let sat = f32(b) / f32(TEST_SEGMENTS - 1u);
                let light = f32(c) / f32(TEST_SEGMENTS - 1u);
                let color = OkHsl(hue, sat, light);
                let xyz = okhsl_into_xyz(color);
                let roundtrip = okhsl_from_xyz(xyz);
                let roundtrip_xyz = okhsl_into_xyz(roundtrip);
                expect_xyz_near(xyz, roundtrip_xyz);
            }
        }
    }
}

@test
fn roundtrip_test_okhsv() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let hue = f32(a) / f32(TEST_SEGMENTS - 1u) * 360.0;
                let sat = f32(b) / f32(TEST_SEGMENTS - 1u);
                let val = f32(c) / f32(TEST_SEGMENTS - 1u);
                let color = OkHsv(hue, sat, val);
                let xyz = okhsv_into_xyz(color);
                let roundtrip = okhsv_from_xyz(xyz);
                let roundtrip_xyz = okhsv_into_xyz(roundtrip);
                expect_xyz_near(xyz, roundtrip_xyz);
            }
        }
    }
}

@test
fn roundtrip_test_rgb() {
    for (var a = 0u; a < TEST_SEGMENTS; a += 1u) {
        for (var b = 0u; b < TEST_SEGMENTS; b += 1u) {
            for (var c = 0u; c < TEST_SEGMENTS; c += 1u) {
                let r = f32(a) / f32(TEST_SEGMENTS - 1u);
                let g = f32(b) / f32(TEST_SEGMENTS - 1u);
                let bl = f32(c) / f32(TEST_SEGMENTS - 1u);
                let color = Rgb(r, g, bl);
                let xyz = rgb_into_xyz(color, SRGB_LINEAR_TO_XYZ);
                let roundtrip = rgb_from_xyz(xyz, XYZ_TO_SRGB_LINEAR);
                let roundtrip_xyz = rgb_into_xyz(roundtrip, SRGB_LINEAR_TO_XYZ);
                expect_xyz_near(xyz, roundtrip_xyz);
            }
        }
    }
}
