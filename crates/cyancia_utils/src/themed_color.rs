use std::panic::Location;

use bevy_color::{Oklcha, Srgba};
use gpui::{App, Rgba};
use gpui_component::{Theme, animation::Lerp};

#[macro_export]
macro_rules! random_oklch {
    ($struct_name:ty, $cx:expr) => {
        random_oklch!($struct_name, $cx, 0u32)
    };
    ($struct_name:ty, $cx:expr, $extra_offset:literal) => {{
        const CH: (f32, f32) = {
            let name = stringify!($struct_name).as_bytes();
            let mut i = 0;
            let mut hash = 0u32;
            while i < name.len() {
                hash = (hash << 5).wrapping_sub(hash) + name[i] as u32;
                i += 1;
            }
            hash = (hash << 5).wrapping_sub(hash) + $extra_offset;

            const MIN_C: f32 = 0.05;
            const MAX_C: f32 = 0.15;
            let c = MIN_C + (hash as f32 / u32::MAX as f32) * (MAX_C - MIN_C);
            let h = (hash % 360) as f32;
            (c, h)
        };

        cyancia_utils::themed_color::themed_oklch(CH.0, CH.1, $cx)
    }};
}

pub fn themed_oklch(c: f32, h: f32, cx: &App) -> Rgba {
    let l = if Theme::global(cx).is_dark() {
        0.4
    } else {
        0.7
    };
    let color = Srgba::from(Oklcha::new(l, c, h, 1.0));
    Rgba {
        r: color.red,
        g: color.green,
        b: color.blue,
        a: color.alpha,
    }
}
