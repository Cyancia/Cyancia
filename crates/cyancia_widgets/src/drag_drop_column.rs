//! Distribute content vertically.
use iced_core::alignment::{self, Alignment};
use iced_core::keyboard::key;
use iced_core::overlay;
use iced_core::renderer;
use iced_core::widget::{Operation, Tree, tree};
use iced_core::{
    Clipboard, Element, Event, Layout, Length, Padding, Pixels, Rectangle, Shell, Size, Vector,
    Widget,
};
use iced_core::{Point, layout};
use iced_core::{keyboard, mouse};

pub struct DragDropContext {
    pub item_index: usize,
    pub absolute_position: Point,
    pub gap_index: usize,
    pub column_bounds: Rectangle,
}

/// A container that distributes its contents vertically.
///
/// # Example
/// ```no_run
/// # mod iced { pub mod widget { pub use iced_widget::*; } }
/// # pub type State = ();
/// # pub type Element<'a, Message> = iced_widget::core::Element<'a, Message, iced_widget::Theme, iced_widget::Renderer>;
/// use iced::widget::{button, column};
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     // ...
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     column![
///         "I am on top!",
///         button("I am in the center!"),
///         "I am below.",
///     ].into()
/// }
/// ```
pub struct DragDropColumn<'a, Message, Theme, Renderer> {
    spacing: f32,
    padding: Padding,
    width: Length,
    height: Length,
    max_width: f32,
    align: Alignment,
    clip: bool,
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    on_grab: Option<Box<dyn Fn(DragDropContext) -> Option<Message>>>,
    on_drag_start: Option<Box<dyn Fn(DragDropContext) -> Option<Message>>>,
    on_drag_update: Option<Box<dyn Fn(DragDropContext) -> Option<Message>>>,
    on_drop: Option<Box<dyn Fn(DragDropContext) -> Option<Message>>>,
    on_drag_cancel: Option<Box<dyn Fn() -> Option<Message>>>,
}

impl<'a, Message, Theme, Renderer> DragDropColumn<'a, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    /// Creates an empty [`Column`].
    pub fn new() -> Self {
        Self::from_vec(Vec::new())
    }

    /// Creates a [`Column`] with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::from_vec(Vec::with_capacity(capacity))
    }

    /// Creates a [`Column`] with the given elements.
    pub fn with_children(
        children: impl IntoIterator<Item = Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        let iterator = children.into_iter();

        Self::with_capacity(iterator.size_hint().0).extend(iterator)
    }

    /// Creates a [`Column`] from an already allocated [`Vec`].
    ///
    /// Keep in mind that the [`Column`] will not inspect the [`Vec`], which means
    /// it won't automatically adapt to the sizing strategy of its contents.
    ///
    /// If any of the children have a [`Length::Fill`] strategy, you will need to
    /// call [`Column::width`] or [`Column::height`] accordingly.
    pub fn from_vec(children: Vec<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            spacing: 0.0,
            padding: Padding::ZERO,
            width: Length::Shrink,
            height: Length::Shrink,
            max_width: f32::INFINITY,
            align: Alignment::Start,
            clip: false,
            children,
            on_grab: None,
            on_drag_start: None,
            on_drag_update: None,
            on_drop: None,
            on_drag_cancel: None,
        }
    }

    /// Sets the vertical spacing _between_ elements.
    ///
    /// Custom margins per element do not exist in iced. You should use this
    /// method instead! While less flexible, it helps you keep spacing between
    /// elements consistent.
    pub fn spacing(mut self, amount: impl Into<Pixels>) -> Self {
        self.spacing = amount.into().0;
        self
    }

    /// Sets the [`Padding`] of the [`Column`].
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the width of the [`Column`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Column`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the maximum width of the [`Column`].
    pub fn max_width(mut self, max_width: impl Into<Pixels>) -> Self {
        self.max_width = max_width.into().0;
        self
    }

    /// Sets the horizontal alignment of the contents of the [`Column`] .
    pub fn align_x(mut self, align: impl Into<alignment::Horizontal>) -> Self {
        self.align = Alignment::from(align.into());
        self
    }

    /// Sets whether the contents of the [`Column`] should be clipped on
    /// overflow.
    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    /// Adds an element to the [`Column`].
    pub fn push(mut self, child: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        let child = child.into();
        let child_size = child.as_widget().size_hint();

        if !child_size.is_void() {
            self.width = self.width.enclose(child_size.width);
            self.height = self.height.enclose(child_size.height);
            self.children.push(child);
        }

        self
    }

    /// Extends the [`Column`] with the given children.
    pub fn extend(
        self,
        children: impl IntoIterator<Item = Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        children.into_iter().fold(self, Self::push)
    }

    pub fn on_grab<F>(mut self, f: F) -> Self
    where
        F: 'static + Fn(DragDropContext) -> Option<Message>,
    {
        self.on_grab = Some(Box::new(f));
        self
    }

    pub fn on_drag_start<F>(mut self, f: F) -> Self
    where
        F: 'static + Fn(DragDropContext) -> Option<Message>,
    {
        self.on_drag_start = Some(Box::new(f));
        self
    }

    pub fn on_drag_update<F>(mut self, f: F) -> Self
    where
        F: 'static + Fn(DragDropContext) -> Option<Message>,
    {
        self.on_drag_update = Some(Box::new(f));
        self
    }

    pub fn on_drop<F>(mut self, f: F) -> Self
    where
        F: 'static + Fn(DragDropContext) -> Option<Message>,
    {
        self.on_drop = Some(Box::new(f));
        self
    }

    pub fn on_drag_cancel<F>(mut self, f: F) -> Self
    where
        F: 'static + Fn() -> Option<Message>,
    {
        self.on_drag_cancel = Some(Box::new(f));
        self
    }
}

impl<Message, Theme, Renderer> Default for DragDropColumn<'_, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Message, Theme, Renderer: iced_core::Renderer>
    FromIterator<Element<'a, Message, Theme, Renderer>>
    for DragDropColumn<'a, Message, Theme, Renderer>
{
    fn from_iter<T: IntoIterator<Item = Element<'a, Message, Theme, Renderer>>>(iter: T) -> Self {
        Self::with_children(iter)
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for DragDropColumn<'_, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.max_width(self.max_width);

        layout::flex::resolve(
            layout::flex::Axis::Vertical,
            renderer,
            &limits,
            self.width,
            self.height,
            self.padding,
            self.spacing,
            self.align,
            &mut self.children,
            &mut tree.children,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .for_each(|((child, state), layout)| {
                    child
                        .as_widget_mut()
                        .operate(state, layout, renderer, operation);
                });
        });
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
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }

        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor_pos) = cursor.position() else {
                    return;
                };

                for (i, child) in layout.children().enumerate() {
                    if child.bounds().contains(cursor_pos) {
                        *state = State::Grabbed {
                            index: i,
                            position: cursor_pos,
                        };
                        if let Some(on_grab) = &self.on_grab
                            && let Some(m) = on_grab(DragDropContext {
                                item_index: i,
                                absolute_position: cursor_pos,
                                gap_index: i,
                                column_bounds: layout.bounds(),
                            })
                        {
                            shell.publish(m);
                        }
                        shell.capture_event();
                        break;
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => match *state {
                State::Idle => {}
                State::Grabbed {
                    index,
                    position: origin,
                } => {
                    let d = position.distance(origin);
                    if d > 8.0 {
                        *state = State::Dragging { index, origin };
                        if let Some(on_drag_update) = &self.on_drag_update
                            && let Some(m) = on_drag_update(DragDropContext {
                                item_index: index,
                                absolute_position: *position,
                                gap_index: find_nearest_gap_index(&layout, *position),
                                column_bounds: layout.bounds(),
                            })
                        {
                            shell.publish(m);
                        }
                        shell.capture_event();
                    }
                }
                State::Dragging { index, origin } => {
                    shell.request_redraw();
                    shell.capture_event();
                }
            },
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => match *state {
                State::Idle => {}
                State::Grabbed { .. } => {
                    *state = State::Idle;
                }
                State::Dragging { index, .. } => {
                    if let Some(on_drop) = &self.on_drop
                        && let Some(cursor_pos) = cursor.position()
                        && let Some(m) = on_drop(DragDropContext {
                            item_index: index,
                            absolute_position: cursor_pos,
                            gap_index: find_nearest_gap_index(&layout, cursor_pos),
                            column_bounds: layout.bounds(),
                        })
                    {
                        shell.publish(m);
                    }
                    shell.request_redraw();
                    shell.capture_event();
                    *state = State::Idle;
                }
            },
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                location,
                modifiers,
                text,
                repeat,
            }) => {
                if *physical_key == key::Physical::Code(key::Code::Escape) {
                    match state {
                        State::Dragging { .. } => {
                            *state = State::Idle;
                            if let Some(on_drag_cancel) = &self.on_drag_cancel
                                && let Some(m) = on_drag_cancel()
                            {
                                shell.publish(m);
                            }
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();

        match *state {
            State::Idle => self
                .children
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
                .map(|((child, tree), layout)| {
                    child
                        .as_widget()
                        .mouse_interaction(tree, layout, cursor, viewport, renderer)
                })
                .max()
                .unwrap_or_default(),
            State::Grabbed { .. } | State::Dragging { .. } => mouse::Interaction::Grabbing,
        }
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
        if let Some(clipped_viewport) = layout.bounds().intersection(viewport) {
            let viewport = if self.clip {
                &clipped_viewport
            } else {
                viewport
            };

            let state = tree.state.downcast_ref::<State>();
            let (dragged_index, origin) = match state {
                State::Idle | State::Grabbed { .. } => (usize::MAX, Point::ORIGIN),
                State::Dragging { index, origin } => (*index, *origin),
            };

            for (((i, child), tree), layout) in self
                .children
                .iter()
                .enumerate()
                .zip(&tree.children)
                .zip(layout.children())
                .filter(|(_, layout)| layout.bounds().intersects(viewport))
            {
                if i == dragged_index
                    && let Some(cursor_pos) = cursor.position()
                {
                    renderer.with_translation(cursor_pos - origin, |renderer| {
                        child
                            .as_widget()
                            .draw(tree, renderer, theme, style, layout, cursor, viewport);
                    });
                } else {
                    child
                        .as_widget()
                        .draw(tree, renderer, theme, style, layout, cursor, viewport);
                }
            }
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<DragDropColumn<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced_core::Renderer + 'a,
{
    fn from(column: DragDropColumn<'a, Message, Theme, Renderer>) -> Self {
        Self::new(column)
    }
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Grabbed {
        index: usize,
        position: Point,
    },
    Dragging {
        index: usize,
        origin: Point,
    },
}

fn find_nearest_gap_index(root: &Layout<'_>, position: Point) -> usize {
    for (i, child) in root.children().enumerate() {
        if child.bounds().y > position.y {
            return i;
        }
    }

    root.children().len()
}
