use iced_core::{Element, Length, Theme};
use iced_wgpu::Renderer;

use crate::{button::Button, flex::Flex};

struct Segment<'a, Message> {
    content: Element<'a, Message, Theme, Renderer>,
    selected: bool,
    message: Option<Message>,
}

pub struct SegmentedControl<'a, Message> {
    segments: Vec<Segment<'a, Message>>,
    width: Length,
    height: Length,
}

impl<'a, Message> SegmentedControl<'a, Message> {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            width: Length::Shrink,
            height: Length::Fixed(24.0),
        }
    }

    pub fn push(
        mut self,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        selected: bool,
        message: Message,
    ) -> Self {
        self.segments.push(Segment {
            content: content.into(),
            selected,
            message: Some(message),
        });
        self
    }

    pub fn push_disabled(
        mut self,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        selected: bool,
    ) -> Self {
        self.segments.push(Segment {
            content: content.into(),
            selected,
            message: None,
        });
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

impl<Message> Default for SegmentedControl<'_, Message> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Message: 'a> From<SegmentedControl<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
{
    fn from(value: SegmentedControl<'a, Message>) -> Self {
        let buttons = value.segments.into_iter().map(|segment| {
            Button::new(segment.content)
                .height(Length::Fill)
                .padding([0, 12])
                .transparent()
                .activated(segment.selected)
                .on_press_maybe(segment.message)
                .into()
        });
        Flex::row(buttons)
            .width(value.width)
            .height(value.height)
            .surface()
            .into()
    }
}
