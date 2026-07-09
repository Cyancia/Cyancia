pub mod gray;
pub mod lab;
pub mod lch;
pub(crate) mod okcolor;
pub mod okhsl;
pub mod okhsv;
pub mod oklab;
pub mod oklch;
pub mod rgb;
pub mod xyz;

#[cfg(test)]
pub(crate) mod tests {
    use std::fmt::Debug;

    use crate::model::xyz::Xyz;

    pub const TEST_SEGMENTS: u32 = 16;
    pub const TEST_EPSILON: f32 = 1e-4;

    pub fn roundtrip_test<T: Copy + Debug>(
        new: impl Fn(f32, f32, f32) -> T,
        to_xyz: impl Fn(T) -> Xyz,
        from_xyz: impl Fn(Xyz) -> T,
    ) {
        for a in 0..TEST_SEGMENTS {
            for b in 0..TEST_SEGMENTS {
                for c in 0..TEST_SEGMENTS {
                    let a0 = (a as f32) / (TEST_SEGMENTS - 1) as f32;
                    let b0 = (b as f32) / (TEST_SEGMENTS - 1) as f32;
                    let c0 = (c as f32) / (TEST_SEGMENTS - 1) as f32;

                    let color = new(a0, b0, c0);
                    let xyz = to_xyz(color);
                    let roundtrip = from_xyz(xyz);
                    let roundtrip_xyz = to_xyz(roundtrip);

                    assert!(
                        (xyz.x - roundtrip_xyz.x).abs() < TEST_EPSILON
                            && (xyz.y - roundtrip_xyz.y).abs() < TEST_EPSILON
                            && (xyz.z - roundtrip_xyz.z).abs() < TEST_EPSILON,
                        "sample=({a0}, {b0}, {c0}) color={color:?} roundtrip={roundtrip:?} xyz={xyz:?} roundtrip_xyz={roundtrip_xyz:?}"
                    );
                }
            }
        }
    }
}
