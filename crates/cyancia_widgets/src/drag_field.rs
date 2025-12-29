use iced_core::{
    Clipboard, Element, Event, Length, Padding, Point, Rectangle, Shell, Size, Vector, Widget,
    layout::{self, Limits},
    mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};

pub struct DragField<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_drag_start: Option<Box<dyn Fn(mouse::Button, Point) -> Option<Message> + 'a>>,
    on_drag: Option<Box<dyn Fn(mouse::Button, Option<Point>) -> Option<Message> + 'a>>,
    on_drag_end: Option<Box<dyn Fn(mouse::Button, Option<Point>) -> Option<Message> + 'a>>,
}

impl<'a, Message, Theme, Renderer> DragField<'a, Message, Theme, Renderer> {
    pub fn new(
        content: Element<'a, Message, Theme, Renderer>,
    ) -> DragField<'a, Message, Theme, Renderer> {
        DragField {
            content,
            on_drag_start: None,
            on_drag: None,
            on_drag_end: None,
        }
    }

    pub fn on_drag_start<F>(mut self, f: F) -> Self
    where
        F: Fn(mouse::Button, Point) -> Option<Message> + 'a,
    {
        self.on_drag_start = Some(Box::new(f));
        self
    }

    pub fn on_drag<F>(mut self, f: F) -> Self
    where
        F: Fn(mouse::Button, Option<Point>) -> Option<Message> + 'a,
    {
        self.on_drag = Some(Box::new(f));
        self
    }

    pub fn on_drag_end<F>(mut self, f: F) -> Self
    where
        F: Fn(mouse::Button, Option<Point>) -> Option<Message> + 'a,
    {
        self.on_drag_end = Some(Box::new(f));
        self
    }
}

#[derive(Default)]
struct State {
    pressed: Option<(mouse::Button, Point, Vector)>,
    current_offset: Vector,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for DragField<'_, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_ref::<State>();
        dbg!(state.current_offset);
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
            .translate(state.current_offset)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: layout::Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(pressed)) => {
                let Some(position) = cursor.position_over(layout.bounds()) else {
                    return;
                };

                state.pressed = Some((*pressed, position, state.current_offset));
                if let Some(on_drag_start) = &self.on_drag_start
                    && let Some(m) = on_drag_start(*pressed, position)
                {
                    shell.publish(m);
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(cursor) = cursor.position()
                    && let Some((pressed, origin, original_offset)) = state.pressed
                {
                    state.current_offset = original_offset + (cursor - origin);
                    if let Some(on_drag) = &self.on_drag {
                        if let Some(m) = on_drag(pressed, Some(cursor)) {
                            shell.publish(m);
                        }
                    }
                };

                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(released)) => {
                let Some((pressed, origin, original_offset)) = state.pressed else {
                    return;
                };
                if pressed != *released {
                    return;
                }

                if let Some(on_drag_end) = &self.on_drag_end
                    && let Some(cursor) = cursor.position()
                    && let Some(m) = on_drag_end(pressed, Some(cursor))
                {
                    shell.publish(m);
                }
                state.pressed = None;
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.pressed.is_some() {
            mouse::Interaction::Grabbing
        } else {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                cursor,
                viewport,
                renderer,
            )
        }
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: layout::Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<DragField<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced_core::Renderer + 'a,
{
    fn from(drag_field: DragField<'a, Message, Theme, Renderer>) -> Self {
        Element::new(drag_field)
    }
}
