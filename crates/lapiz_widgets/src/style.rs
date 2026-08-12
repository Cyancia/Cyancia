use iced_core::Theme;
use iced_widget::{Button, button};

pub trait ButtonStyle {
    fn style_pressed(self, pressed: bool) -> Self;
}

impl<'a, Message, Renderer> ButtonStyle for Button<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn style_pressed(self, pressed: bool) -> Self {
        if pressed {
            self.style(button::primary)
        } else {
            self.style(button::secondary)
        }
    }
}
