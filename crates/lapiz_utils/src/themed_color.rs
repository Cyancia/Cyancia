use bevy_color::{Oklcha, Srgba};
use iced_core::Color;

#[macro_export]
macro_rules! random_oklch_hue_chroma {
    ($struct_name:ty) => {{
        const CH: (f32, f32) = {
            let name = stringify!($struct_name).as_bytes();
            let mut i = 0;
            let mut hash = 0u32;
            while i < name.len() {
                hash = (hash << 5).wrapping_sub(hash) + name[i] as u32;
                i += 1;
            }

            const MIN_C: f32 = 0.05;
            const MAX_C: f32 = 0.15;
            let h = (hash % 360) as f32;
            let c = MIN_C + (hash as f32 / u32::MAX as f32) * (MAX_C - MIN_C);
            (h, c)
        };

        CH
    }};
}

pub fn themed_oklch(c: f32, h: f32, is_dark: bool) -> Color {
    let l = if is_dark { 0.4 } else { 0.7 };
    let color = Srgba::from(Oklcha::new(l, c, h, 1.0));
    Color::from_rgba(
        color.red.clamp(0.0, 1.0),
        color.green.clamp(0.0, 1.0),
        color.blue.clamp(0.0, 1.0),
        color.alpha.clamp(0.0, 1.0),
    )
}
