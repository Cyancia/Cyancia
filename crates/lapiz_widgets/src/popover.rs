use iced_core::{
    Clipboard, Element, Event, Layout, Length, Point, Rectangle, Shell, Size, Vector, Widget,
    layout::{Limits, Node},
    mouse, overlay, renderer,
    widget::{Operation, Tree},
};

pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

pub struct Popover<'a, Message, Theme, Renderer> {
    trigger: Element<'a, Message, Theme, Renderer>,
    content: Option<Element<'a, Message, Theme, Renderer>>,
    anchor: Anchor,
}

impl<'a, Message, Theme, Renderer> Popover<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    pub fn new(trigger: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            trigger: trigger.into(),
            content: None,
            anchor: Anchor::BottomLeft,
        }
    }

    /// Sets the floating panel. `None` hides the overlay.
    pub fn content(mut self, content: Option<Element<'a, Message, Theme, Renderer>>) -> Self {
        self.content = content;
        self
    }

    /// Offsets the floating panel relative to the trigger's top-left corner.
    pub fn offset(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Popover<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        let mut children = vec![Tree::new(&self.trigger)];
        if let Some(content) = &self.content {
            children.push(Tree::new(content));
        }
        children
    }

    fn diff(&self, tree: &mut Tree) {
        let mut children = vec![&self.trigger];
        if let Some(content) = &self.content {
            children.push(content);
        }
        tree.diff_children(&children);
    }

    fn size(&self) -> Size<Length> {
        self.trigger.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.trigger
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.trigger
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.trigger.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.trigger.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.trigger.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = tree.children.iter_mut();
        let trigger_overlay = self.trigger.as_widget_mut().overlay(
            children.next()?,
            layout,
            renderer,
            viewport,
            translation,
        );

        let content_overlay = self.content.as_mut().map(|content| {
            let resolved_offset_rel = match self.anchor {
                Anchor::TopLeft => Vector::new(0.0, 0.0),
                Anchor::TopCenter => Vector::new(0.5, 0.0),
                Anchor::TopRight => Vector::new(1.0, 0.0),
                Anchor::BottomLeft => Vector::new(0.0, 1.0),
                Anchor::BottomCenter => Vector::new(0.5, 1.0),
                Anchor::BottomRight => Vector::new(1.0, 1.0),
            };

            let offset = layout.bounds().size() * resolved_offset_rel;
            overlay::Element::new(Box::new(Overlay {
                position: layout.bounds().position() + translation,
                offset: Vector::new(offset.width, offset.height),
                content,
                tree: children.next().unwrap(),
            }))
        });

        match (trigger_overlay, content_overlay) {
            (Some(trigger), Some(content)) => {
                Some(overlay::Group::with_children(vec![trigger, content]).overlay())
            }
            (Some(trigger), None) => Some(trigger),
            (None, content @ Some(_)) => content,
            (None, None) => None,
        }
    }
}

impl<'a, Message, Theme, Renderer> From<Popover<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer + 'a,
    Theme: 'a,
    Message: 'a,
{
    fn from(popover: Popover<'a, Message, Theme, Renderer>) -> Self {
        Element::new(popover)
    }
}

struct Overlay<'a, 'b, Message, Theme, Renderer> {
    position: Point,
    offset: Vector,
    content: &'b mut Element<'a, Message, Theme, Renderer>,
    tree: &'b mut Tree,
}

impl<'a, 'b, Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for Overlay<'a, 'b, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> Node {
        let limits = Limits::new(Size::ZERO, Size::new(bounds.width, f32::INFINITY));
        let node = self
            .content
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        node.move_to(Point::new(
            self.position.x + self.offset.x,
            self.position.y + self.offset.y,
        ))
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let viewport = layout.bounds();
        self.content.as_widget_mut().update(
            self.tree, event, layout, cursor, renderer, clipboard, shell, &viewport,
        );
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.content
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &layout.bounds(),
            Vector::ZERO,
        )
    }
}
