use cyancia_shader_graph::{
    ShaderGraph, ShaderGraphDefaultInputSlot, ShaderGraphDefaultOutputSlot, ShaderGraphNode,
    ShaderGraphNodeCodeGenContext, ShaderGraphRenderer, ShaderGraphTheme, ShaderGraphValueType,
    editor::{GraphView, GraphViewMessage},
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
    graph: ShaderGraph,
    // creators: Vec<Box<dyn GraphNodeCreator>>,
    // viewers: GraphSlotViewers<'static, GraphMessage, Theme, Renderer>,
}

#[derive(Debug)]
pub enum GraphMessage {
    // NodeDrawer(NodeDrawerMessage),
    // FloatValueChanged(f32, InputSlotId),
    View(GraphViewMessage),
}

impl App {
    pub fn new() -> Self {
        let mut graph = ShaderGraph::default();
        let add1 = graph.add_node(Point::new(0.0, 0.0), AddNode);
        let add2 = graph.add_node(Point::new(200.0, 0.0), AddNode);
        graph.connect_slots_by_index(add1, 0, add2, 0);
        // let viewers = {
        //     let mut v = GraphSlotViewers::new();
        //     v.register(FloatType);
        //     v
        // };

        Self {
            graph,
            // creators: vec![Box::new(AddNodeCreator)],
            // viewers,
        }
    }

    // pub fn view(&self) -> Element<'_, GraphEditorMessage<GraphMessage>> {
    pub fn view(&self) -> Element<'_, GraphMessage, ShaderGraphTheme, ShaderGraphRenderer> {
        Element::new(GraphView::new(&self.graph)).map(GraphMessage::View)
        // container("content").into()
        // row![
        //     node_drawer(&self.creators)
        //         .map(GraphMessage::NodeDrawer)
        //         .map(GraphEditorMessage::Custom),
        //     // column![
        //     //     Text::new("test1"),
        //     //     Text::new("test1"),
        //     //     Text::new("test1"),
        //     //     Text::new("test2"),
        //     //     DragField::new(
        //     //         Text::new("Drag me!").into()
        //     //     ),
        //     //     Text::new("test1"),
        //     // ]
        //     Element::new(GraphView::new(&self.graph, &self.viewers,))
        // ]
        // .into()
    }

    pub fn update(&mut self, message: GraphMessage) {
        match message {
            GraphMessage::View(message) => match message {
                GraphViewMessage::NodeMoved(point, id) => {
                    if let Some(node) = self.graph.get_node_mut(&id) {
                        node.position = point;
                    }
                }
                GraphViewMessage::EdgeCreated(from, to) => {
                    self.graph.connect_slot(from, to);
                }
                GraphViewMessage::EdgeRemoved(id) => {
                    self.graph.disconnect_slot(id);
                }
                GraphViewMessage::SlotValue(message) => {
                    self.graph.update_literal(message);
                }
            },
        }
        println!("{}", self.graph.compile().unwrap());
        // match message {
        //     GraphEditorMessage::NodeMoved(point, node_id) => {
        //         if let Some(node) = self.graph.nodes.get_mut(&node_id) {
        //             node.position = point;
        //         }
        //     }
        //     GraphEditorMessage::Custom(message) => match message {
        //         GraphMessage::FloatValueChanged(x, id) => {
        //             if let Some(slot) = self.graph.slots.inputs.get_mut(&id) {
        //                 slot.value = ErasedSlotValue::new(x);
        //             }
        //         }
        //         GraphMessage::NodeDrawer(message) => match message {
        //             NodeDrawerMessage::NodeCreate(i, point) => {
        //                 self.graph.add_node(point, self.creators[i].create());
        //             }
        //         },
        //     },
        //     GraphEditorMessage::EdgeCreated(from, to) => {
        //         self.graph.connect_slot(from, to);
        //     }
        //     GraphEditorMessage::EdgeRemoved(to) => {
        //         self.graph.disconnect_slot(to);
        //     }
        // }
    }
}

#[derive(Default)]
pub struct FloatType;

#[derive(Debug, Clone)]
pub enum FloatTypeMessage {
    ValueChanged(f32),
}

impl ShaderGraphValueType for FloatType {
    type AssociatedLiteralType = f32;

    type Message = FloatTypeMessage;

    fn color(&self) -> Color {
        Color::from_rgb8(100, 200, 100)
    }

    fn name(&self) -> &'static str {
        "Float"
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, ShaderGraphTheme, ShaderGraphRenderer> {
        SpinSlider::new(0.0..=1.0, *data, |x| FloatTypeMessage::ValueChanged(x))
            .step(0.01)
            .into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        match message {
            FloatTypeMessage::ValueChanged(x) => {
                *data = x;
            }
        }
    }

    fn literal_to_string(&self, data: &Self::AssociatedLiteralType) -> String {
        format!("{:.5}", data)
    }
}

#[derive(Default)]
pub struct AddNode;

impl ShaderGraphNode for AddNode {
    fn title(&self) -> &str {
        "Add"
    }

    fn title_color(&self) -> Color {
        Color::from_rgb8(200, 100, 100)
    }

    fn create_inputs(&self) -> Vec<ShaderGraphDefaultInputSlot> {
        vec![
            ShaderGraphDefaultInputSlot::new::<FloatType>("A", 0.0),
            ShaderGraphDefaultInputSlot::new::<FloatType>("B", 0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<ShaderGraphDefaultOutputSlot> {
        vec![ShaderGraphDefaultOutputSlot::new::<FloatType>("Result")]
    }

    fn generate_code(&self, ctx: ShaderGraphNodeCodeGenContext) -> String {
        let a = ctx.get_input::<0>().unwrap();
        let b = ctx.get_input::<1>().unwrap();
        let output = ctx.get_output::<0>().unwrap();
        format!("let {} = {} + {};", output, a, b)
    }
}
