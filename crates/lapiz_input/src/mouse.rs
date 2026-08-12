use iced_core::Point;

#[derive(Debug, Clone)]
pub struct PressedMouseState {
    pub position: Point,
}

#[derive(Debug, Clone)]
pub struct HoverMouseState {
    pub position: Point,
}
