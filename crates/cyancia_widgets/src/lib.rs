use gpui::App;

pub mod curve_edit;

pub fn init(cx: &mut App) {
    curve_edit::init(cx);
}
