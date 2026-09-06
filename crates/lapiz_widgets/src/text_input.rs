use iced_core::{Background, Border, Element, Length, Padding, Pixels, Theme, alignment};
use iced_wgpu::Renderer;

pub use iced_widget::text_input::{Catalog, Icon, Status, Style, StyleFn};

pub struct TextInput<'a, Message> {
    inner: iced_widget::TextInput<'a, Message, Theme, Renderer>,
}

impl<'a, Message: Clone> TextInput<'a, Message> {
    pub fn new(placeholder: &str, value: &str) -> Self {
        Self {
            inner: iced_widget::TextInput::new(placeholder, value)
                .size(12)
                .style(default),
        }
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.inner = self.inner.secure(secure);
        self
    }

    pub fn on_input(mut self, f: impl Fn(String) -> Message + 'a) -> Self {
        self.inner = self.inner.on_input(f);
        self
    }

    pub fn on_input_maybe<F>(mut self, f: Option<F>) -> Self
    where
        F: Fn(String) -> Message + 'a,
    {
        self.inner = self.inner.on_input_maybe(f);
        self
    }

    pub fn on_submit(mut self, message: Message) -> Self {
        self.inner = self.inner.on_submit(message);
        self
    }

    pub fn on_submit_maybe(mut self, message: Option<Message>) -> Self {
        self.inner = self.inner.on_submit_maybe(message);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.inner = self.inner.padding(padding);
        self
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.inner = self.inner.size(size);
        self
    }

    pub fn align_x(mut self, alignment: impl Into<alignment::Horizontal>) -> Self {
        self.inner = self.inner.align_x(alignment);
        self
    }

    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    pub fn class(mut self, class: impl Into<<Theme as Catalog>::Class<'a>>) -> Self {
        self.inner = self.inner.class(class);
        self
    }

    pub fn transparent(self) -> Self {
        self.style(transparent)
    }

    pub fn invalid(self) -> Self {
        self.style(invalid)
    }

    pub fn compact(self) -> Self {
        self.padding([3, 8]).size(12)
    }
}

impl<'a, Message: Clone + 'a> From<TextInput<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
{
    fn from(value: TextInput<'a, Message>) -> Self {
        value.inner.into()
    }
}

pub fn default(theme: &Theme, status: Status) -> Style {
    let p = theme.extended_palette();
    let focused = matches!(status, Status::Focused { .. });
    let hovered = matches!(
        status,
        Status::Hovered | Status::Focused { is_hovered: true }
    );
    let disabled = matches!(status, Status::Disabled);
    let mut style = Style {
        background: Background::Color(p.background.base.color),
        border: Border {
            radius: 0.0.into(),
            width: 1.0,
            color: if focused {
                p.primary.base.color
            } else if hovered {
                p.background.stronger.color
            } else {
                p.background.strong.color
            },
        },
        icon: p.background.weak.text,
        placeholder: p.background.weak.text,
        value: p.background.base.text,
        selection: p.primary.weak.color,
    };
    if disabled {
        if let Background::Color(ref mut color) = style.background {
            color.a *= 0.5;
        }
        style.value.a *= 0.4;
        style.placeholder.a *= 0.4;
        style.border.color.a *= 0.4;
    }
    style
}

pub fn transparent(theme: &Theme, status: Status) -> Style {
    Style {
        background: Background::Color(iced_core::Color::TRANSPARENT),
        ..default(theme, status)
    }
}

pub fn invalid(theme: &Theme, status: Status) -> Style {
    let mut style = default(theme, status);
    style.border.color = theme.extended_palette().danger.base.color;
    style
}
