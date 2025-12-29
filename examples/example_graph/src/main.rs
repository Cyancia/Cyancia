use cyancia_graph::{
    DefaultGraphSlot, ErasedSlotValue, Graph, GraphError, GraphNode, GraphNodeCreator,
    GraphNodeSlotsContext, GraphSlotValueType, InputSlotId,
    editor::{
        DrawableGraph, GraphEditorMessage, GraphSlotId, GraphSlotViewer, GraphSlotViewers,
        GraphView,
        drawer::{NodeDrawerMessage, node_drawer},
        helpers::{empty_slot, valued_slot},
    },
};
use cyancia_utils::wrapper;
use cyancia_widgets::{drag_field::DragField, spin_slider::SpinSlider};
use iced::{
    Color, Element,
    Length::Fill,
    Point, Renderer, Theme,
    advanced::{Widget, layout},
    widget::{Text, column, container, row, sensor},
};

fn main() {
    iced::application(App::new, App::update, App::view)
        .run()
        .unwrap();
}

pub struct App {
    graph: Graph,
    creators: Vec<Box<dyn GraphNodeCreator>>,
    viewers: GraphSlotViewers<'static, GraphMessage, Theme, Renderer>,
}

#[derive(Debug, Clone)]
pub enum GraphMessage {
    NodeDrawer(NodeDrawerMessage),
    FloatValueChanged(f32, InputSlotId),
}

impl App {
    pub fn new() -> Self {
        let mut graph = Graph::new();
        // let add1 = graph.add_node(Point::new(0.0, 0.0), AddNode);
        // let add2 = graph.add_node(Point::new(200.0, 0.0), AddNode);
        // graph.connect_slots_by_index(add1, 0, add2, 0);
        let viewers = {
            let mut v = GraphSlotViewers::new();
            v.register(FloatType);
            v
        };

        Self {
            graph,
            creators: vec![Box::new(AddNodeCreator)],
            viewers,
        }
    }

    pub fn view(&self) -> Element<'_, GraphEditorMessage<GraphMessage>> {
        row![
            node_drawer(&self.creators)
                .map(GraphMessage::NodeDrawer)
                .map(GraphEditorMessage::Custom),
            // column![
            //     Text::new("test1"),
            //     Text::new("test1"),
            //     Text::new("test1"),
            //     Text::new("test2"),
            //     DragField::new(
            //         Text::new("Drag me!").into()
            //     ),
            //     Text::new("test1"),
            // ]
            Element::new(GraphView::new(&self.graph, &self.viewers,))
        ]
        .into()
    }

    pub fn update(&mut self, message: GraphEditorMessage<GraphMessage>) {
        match message {
            GraphEditorMessage::NodeMoved(point, node_id) => {
                if let Some(node) = self.graph.nodes.get_mut(&node_id) {
                    node.position = point;
                }
            }
            GraphEditorMessage::Custom(message) => match message {
                GraphMessage::FloatValueChanged(x, id) => {
                    if let Some(slot) = self.graph.slots.inputs.get_mut(&id) {
                        slot.value = ErasedSlotValue::new(x);
                    }
                }
                GraphMessage::NodeDrawer(message) => match message {
                    NodeDrawerMessage::NodeCreate(i, point) => {
                        self.graph.add_node(point, self.creators[i].create());
                    }
                },
            },
            GraphEditorMessage::EdgeCreated(from, to) => {
                self.graph.connect_slot(from, to);
            }
            GraphEditorMessage::EdgeRemoved(to) => {
                self.graph.disconnect_slot(to);
            }
        }
    }
}

pub struct FloatType;

impl GraphSlotValueType for FloatType {
    fn color(&self) -> Color {
        Color::from_rgb8(255, 0, 0)
    }

    fn type_name(&self) -> &'static str {
        "Float"
    }
}

impl<'a> GraphSlotViewer<'a, GraphMessage, Theme, Renderer> for FloatType {
    fn view(
        &self,
        name: &'static str,
        value: &ErasedSlotValue,
        slot_id: GraphSlotId,
    ) -> Option<Element<'a, GraphMessage, Theme, Renderer>> {
        let GraphSlotId::Input(id) = slot_id else {
            return None;
        };

        let x = value.as_ref::<f32>()?;
        Some(
            SpinSlider::new(0.0..=1f32, *x, move |val| {
                GraphMessage::FloatValueChanged(val, id)
            })
            .width(Fill)
            .step(0.01)
            .into(),
        )
    }
}

pub struct AddNodeCreator;

impl GraphNodeCreator for AddNodeCreator {
    fn name(&self) -> &'static str {
        "Add"
    }

    fn create(&self) -> Box<dyn GraphNode> {
        Box::new(AddNode)
    }
}

pub struct AddNode;

impl GraphNode for AddNode {
    fn header_color(&self) -> Color {
        Color::from_rgb8(0, 150, 150)
    }

    fn name(&self) -> &'static str {
        "Add"
    }

    fn crate_inputs(&self) -> Vec<DefaultGraphSlot> {
        vec![
            DefaultGraphSlot {
                name: "A",
                value_type: Box::new(FloatType),
                value: ErasedSlotValue::new(0.0f32),
            },
            DefaultGraphSlot {
                name: "B",
                value_type: Box::new(FloatType),
                value: ErasedSlotValue::new(0.0f32),
            },
        ]
    }

    fn crate_outputs(&self) -> Vec<DefaultGraphSlot> {
        vec![DefaultGraphSlot {
            name: "Result",
            value_type: Box::new(FloatType),
            value: ErasedSlotValue::empty::<f32>(),
        }]
    }

    fn run(&self, mut slots: GraphNodeSlotsContext<'_>) -> Result<(), GraphError> {
        let a = slots.get_input::<0, f32>()?;
        let b = slots.get_input::<1, f32>()?;
        slots.set_output::<0, f32>(a + b)
    }
}
