use gpui::App;

pub mod curve_edit;
pub mod spin_slider;

pub fn init(cx: &mut App) {
    curve_edit::init(cx);
    spin_slider::init(cx);
}
