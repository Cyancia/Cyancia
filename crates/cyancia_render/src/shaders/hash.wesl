// Hash without Sine
// MIT License...
/* Copyright (c)2014 David Hoskins.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.*/

//----------------------------------------------------------------------------------------

// The name "hash[Out][In]" describes the function signature:
// * EXAMPLES:
// * hash12: f32  = hash(vec2f)  // 1 component out, 2 components in
// * hash21: vec2f = hash(f32)   // 2 components out, 1 component in
// * hash22: vec2f = hash(vec2f) // 2 components out, 2 components in
// * hash33: vec3f = hash(vec3f) // 3 components out, 3 components in

//----------------------------------------------------------------------------------------
//  1 out, 1 in...
fn hash11(p_in: f32) -> f32 {
    var p = fract(p_in * 0.1031);
    p *= p + 33.33;
    p *= p + p;
    return fract(p);
}

//----------------------------------------------------------------------------------------
//  1 out, 2 in...
fn hash12(p: vec2f) -> f32 {
    var p3 = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, vec3f(p3.y, p3.z, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

//----------------------------------------------------------------------------------------
//  1 out, 3 in...
fn hash13(p3_in: vec3f) -> f32 {
    var p3 = fract(p3_in * 0.1031);
    p3 += dot(p3, vec3f(p3.z, p3.y, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

//----------------------------------------------------------------------------------------
// 1 out, 4 in...
fn hash14(p4_in: vec4f) -> f32 {
    var p4 = fract(p4_in * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p4 += dot(p4, vec4f(p4.w, p4.z, p4.x, p4.y) + 33.33);
    return fract((p4.x + p4.y) * (p4.z + p4.w));
}

//----------------------------------------------------------------------------------------
//  2 out, 1 in...
fn hash21(p: f32) -> vec2f {
    var p3 = fract(vec3f(p) * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, vec3f(p3.y, p3.z, p3.x) + 33.33);
    return fract((vec2f(p3.x, p3.x) + vec2f(p3.y, p3.z)) * vec2f(p3.z, p3.y));
}

//----------------------------------------------------------------------------------------
//  2 out, 2 in...
fn hash22(p: vec2f) -> vec2f {
    var p3 = fract(vec3f(p.x, p.y, p.x) * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, vec3f(p3.y, p3.z, p3.x) + 33.33);
    return fract((vec2f(p3.x, p3.x) + vec2f(p3.y, p3.z)) * vec2f(p3.z, p3.y));
}

//----------------------------------------------------------------------------------------
//  2 out, 3 in...
fn hash23(p3_in: vec3f) -> vec2f {
    var p3 = fract(p3_in * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, vec3f(p3.y, p3.z, p3.x) + 33.33);
    return fract((vec2f(p3.x, p3.x) + vec2f(p3.y, p3.z)) * vec2f(p3.z, p3.y));
}

//----------------------------------------------------------------------------------------
//  3 out, 1 in...
fn hash31(p: f32) -> vec3f {
    var p3 = fract(vec3f(p) * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, vec3f(p3.y, p3.z, p3.x) + 33.33);
    return fract((vec3f(p3.x, p3.x, p3.y) + vec3f(p3.y, p3.z, p3.z)) * vec3f(p3.z, p3.y, p3.x));
}

//----------------------------------------------------------------------------------------
//  3 out, 2 in...
fn hash32(p: vec2f) -> vec3f {
    var p3 = fract(vec3f(p.x, p.y, p.x) * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, vec3f(p3.y, p3.x, p3.z) + 33.33);
    return fract((vec3f(p3.x, p3.x, p3.y) + vec3f(p3.y, p3.z, p3.z)) * vec3f(p3.z, p3.y, p3.x));
}

//----------------------------------------------------------------------------------------
//  3 out, 3 in...
fn hash33(p3_in: vec3f) -> vec3f {
    var p3 = fract(p3_in * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, vec3f(p3.y, p3.x, p3.z) + 33.33);
    return fract((vec3f(p3.x, p3.x, p3.y) + vec3f(p3.y, p3.x, p3.x)) * vec3f(p3.z, p3.y, p3.x));
}

//----------------------------------------------------------------------------------------
// 4 out, 1 in...
fn hash41(p: f32) -> vec4f {
    var p4 = fract(vec4f(p) * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p4 += dot(p4, vec4f(p4.w, p4.z, p4.x, p4.y) + 33.33);
    return fract((vec4f(p4.x, p4.x, p4.y, p4.z) + vec4f(p4.y, p4.z, p4.z, p4.w)) * vec4f(p4.z, p4.y, p4.w, p4.x));
}

//----------------------------------------------------------------------------------------
// 4 out, 2 in...
fn hash42(p: vec2f) -> vec4f {
    var p4 = fract(vec4f(p.x, p.y, p.x, p.y) * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p4 += dot(p4, vec4f(p4.w, p4.z, p4.x, p4.y) + 33.33);
    return fract((vec4f(p4.x, p4.x, p4.y, p4.z) + vec4f(p4.y, p4.z, p4.z, p4.w)) * vec4f(p4.z, p4.y, p4.w, p4.x));
}

//----------------------------------------------------------------------------------------
// 4 out, 3 in...
fn hash43(p: vec3f) -> vec4f {
    var p4 = fract(vec4f(p.x, p.y, p.z, p.x) * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p4 += dot(p4, vec4f(p4.w, p4.z, p4.x, p4.y) + 33.33);
    return fract((vec4f(p4.x, p4.x, p4.y, p4.z) + vec4f(p4.y, p4.z, p4.z, p4.w)) * vec4f(p4.z, p4.y, p4.w, p4.x));
}

//----------------------------------------------------------------------------------------
// 4 out, 4 in...
fn hash44(p4_in: vec4f) -> vec4f {
    var p4 = fract(p4_in * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p4 += dot(p4, vec4f(p4.w, p4.z, p4.x, p4.y) + 33.33);
    return fract((vec4f(p4.x, p4.x, p4.y, p4.z) + vec4f(p4.y, p4.z, p4.z, p4.w)) * vec4f(p4.z, p4.y, p4.w, p4.x));
}