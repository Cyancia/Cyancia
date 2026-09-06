use iced_core::{Element, Length, Theme};
use iced_wgpu::Renderer;

use crate::{
    button::icon_button, callback::Callback, divider::Divider, flex::Flex, icon, label::Label,
    panel::Panel,
};

pub struct Dialog<'a, Message> {
    title: String,
    content: Element<'a, Message, Theme, Renderer>,
    actions: Vec<Element<'a, Message, Theme, Renderer>>,
    close: Callback<'a, Message>,
    width: Length,
}

impl<'a, Message> Dialog<'a, Message> {
    pub fn new(
        title: impl Into<String>,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            actions: Vec::new(),
            close: Callback::Empty,
            width: Length::Fixed(420.0),
        }
    }

    pub fn action(mut self, action: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        self.actions.push(action.into());
        self
    }

    crate::callback_methods!(close);

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
}

impl<'a, Message: 'a> From<Dialog<'a, Message>> for Element<'a, Message, Theme, Renderer> {
    fn from(value: Dialog<'a, Message>) -> Self {
        let header = Flex::row([
            Label::new(value.title).strong().into(),
            icon_button(icon::close().size(13))
                .on_press_with_callback(value.close)
                .into(),
        ])
        .width(Length::Fill)
        .height(32)
        .padding([0, 10]);
        let mut sections = vec![header.into(), Divider::horizontal(1).into(), value.content];
        if !value.actions.is_empty() {
            sections.extend([
                Divider::horizontal(1).into(),
                Flex::row(value.actions)
                    .width(Length::Fill)
                    .height(38)
                    .padding([6, 10])
                    .gap(6)
                    .into(),
            ]);
        }
        Panel::new(Flex::column(sections).width(Length::Fill))
            .width(value.width)
            .into()
    }
}
