use std::fmt::Display;

use cyancia_shader_graph::{
    ErasedShaderGraphNodeCreator, ShaderGraph, ShaderGraphDefaultInputSlot,
    ShaderGraphDefaultOutputSlot, ShaderGraphNode, ShaderGraphNodeCodeGenContext,
    ShaderGraphNodeCreator, ShaderGraphRenderer, ShaderGraphTheme, ShaderGraphValueType,
    ShaderVariable, ShaderVariableCaster,
    editor::{
        GraphView, GraphViewMessage,
        drawer::{NodeDrawerMessage, node_drawer},
    },
};
use cyancia_utils::wrapper;
use cyancia_widgets::{drag_field::DragField, spin_slider::SpinSlider};
use glam::Vec2;
use iced::{
    Color, Element,
    Length::Fill,
    Point, Renderer, Theme,
    advanced::{Widget, layout},
    widget::{Text, column, combo_box, container, pick_list, row, sensor},
};

fn main() {
    iced::application(App::new, App::update, App::view)
        .run()
        .unwrap();
}

pub struct App {
    graph: ShaderGraph,
    creators: Vec<Box<dyn ErasedShaderGraphNodeCreator>>,
}

#[derive(Debug)]
pub enum GraphMessage {
    View(GraphViewMessage),
    NodeDrawer(NodeDrawerMessage),
}

impl App {
    pub fn new() -> Self {
        let mut graph = ShaderGraph::default();
        let add1 = graph.add_node(Point::new(0.0, 0.0), AddNode);
        let add2 = graph.add_node(Point::new(200.0, 0.0), AddNode);
        graph.connect_slots_by_index(add1, 0, add2, 0);
        graph.add_caster::<Vector2DToFloatCaster>();

        Self {
            graph,
            creators: vec![
                Box::new(AddNode),
                Box::new(Vector2DAddNode),
                Box::new(MathNode),
            ],
            // creators: vec![Box::new(AddNodeCreator)],
            // viewers,
        }
    }

    // pub fn view(&self) -> Element<'_, GraphEditorMessage<GraphMessage>> {
    pub fn view(&self) -> Element<'_, GraphMessage, ShaderGraphTheme, ShaderGraphRenderer> {
        row![
            node_drawer(&self.creators).map(GraphMessage::NodeDrawer),
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
            Element::new(GraphView::new(&self.graph)).map(GraphMessage::View)
        ]
        .into()
    }

    pub fn update(&mut self, message: GraphMessage) {
        match message {
            GraphMessage::View(message) => match message {
                GraphViewMessage::NodeMoveRequest(point, id) => {
                    if let Some(node) = self.graph.get_node_mut(&id) {
                        node.position = point;
                    }
                }
                GraphViewMessage::EdgeCreateRequest(from, to) => {
                    self.graph.connect_slots(from, to);
                }
                GraphViewMessage::EdgeRemoveRequest(id) => {
                    self.graph.disconnect_slot(id);
                }
                GraphViewMessage::LiteralUpdate(message) => {
                    self.graph.update_literal(message);
                }
            },
            GraphMessage::NodeDrawer(message) => match message {
                NodeDrawerMessage::NodeCreate(creator, point) => {
                    self.graph
                        .add_boxed_node(point, self.creators[creator].create());
                }
            },
        }
        println!("{}", self.graph.compile().unwrap());
    }
}

#[derive(Default)]
pub struct FloatType;

impl ShaderGraphValueType for FloatType {
    type AssociatedLiteralType = f32;

    type Message = f32;

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
        SpinSlider::new(0.0..=1.0, *data, |x| x).step(0.01).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }

    fn literal_to_string(&self, data: &Self::AssociatedLiteralType) -> String {
        format!("{:.5}", data)
    }
}

#[derive(Default)]
pub struct Vector2DType;

#[derive(Clone)]
pub enum Vector2DTypeMessage {
    XChanged(f32),
    YChanged(f32),
}

impl ShaderGraphValueType for Vector2DType {
    type AssociatedLiteralType = Vec2;

    type Message = Vector2DTypeMessage;

    fn color(&self) -> Color {
        Color::from_rgb8(100, 100, 200)
    }

    fn name(&self) -> &'static str {
        "Vector2D"
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, ShaderGraphTheme, ShaderGraphRenderer> {
        column![
            SpinSlider::new(0.0..=1.0, data.x, |x| Vector2DTypeMessage::XChanged(x)).step(0.01),
            SpinSlider::new(0.0..=1.0, data.y, |x| Vector2DTypeMessage::YChanged(x)).step(0.01)
        ]
        .into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        match message {
            Vector2DTypeMessage::XChanged(x) => {
                data.x = x;
            }
            Vector2DTypeMessage::YChanged(y) => {
                data.y = y;
            }
        }
    }

    fn literal_to_string(&self, data: &Self::AssociatedLiteralType) -> String {
        format!("vec2f({:.2}, {:.2})", data.x, data.y)
    }
}

#[derive(Default)]
pub struct AddNode;

impl ShaderGraphNodeCreator for AddNode {
    type NodeType = Self;
}

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

#[derive(Default)]
pub struct Vector2DAddNode;

impl ShaderGraphNodeCreator for Vector2DAddNode {
    type NodeType = Self;
}

impl ShaderGraphNode for Vector2DAddNode {
    fn title(&self) -> &str {
        "Vector2D Add"
    }

    fn title_color(&self) -> Color {
        Color::from_rgb8(100, 100, 200)
    }

    fn create_inputs(&self) -> Vec<ShaderGraphDefaultInputSlot> {
        vec![
            ShaderGraphDefaultInputSlot::new::<Vector2DType>("A", Vec2::ZERO),
            ShaderGraphDefaultInputSlot::new::<Vector2DType>("B", Vec2::ZERO),
        ]
    }

    fn create_outputs(&self) -> Vec<ShaderGraphDefaultOutputSlot> {
        vec![ShaderGraphDefaultOutputSlot::new::<Vector2DType>("Result")]
    }

    fn generate_code(&self, ctx: ShaderGraphNodeCodeGenContext) -> String {
        let a = ctx.get_input::<0>().unwrap();
        let b = ctx.get_input::<1>().unwrap();
        let output = ctx.get_output::<0>().unwrap();
        format!("let {} = {} + {};", output, a, b)
    }
}

#[derive(Default)]
pub struct Vector2DToFloatCaster;

impl ShaderVariableCaster for Vector2DToFloatCaster {
    type FromType = Vector2DType;

    type ToType = FloatType;

    fn cast(&self, variable: &String) -> String {
        format!("{}.x", variable)
    }
}

#[derive(Default)]
pub struct MathNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MathNodeMode {
    #[default]
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Display for MathNodeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MathNodeMode::Add => write!(f, "Add"),
            MathNodeMode::Subtract => write!(f, "Subtract"),
            MathNodeMode::Multiply => write!(f, "Multiply"),
            MathNodeMode::Divide => write!(f, "Divide"),
        }
    }
}

impl ShaderGraphValueType for MathNodeMode {
    type AssociatedLiteralType = MathNodeMode;

    type Message = MathNodeMode;

    fn color(&self) -> Color {
        Color::TRANSPARENT
    }

    fn name(&self) -> &'static str {
        "Math Node Mode"
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, ShaderGraphTheme, ShaderGraphRenderer> {
        pick_list(
            vec![
                MathNodeMode::Add,
                MathNodeMode::Subtract,
                MathNodeMode::Multiply,
                MathNodeMode::Divide,
            ],
            Some(*data),
            |mode| mode,
        )
        .into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }

    fn literal_to_string(&self, _data: &Self::AssociatedLiteralType) -> String {
        panic!("This doesn't make sense and should never be called.")
    }
}

impl ShaderGraphNodeCreator for MathNode {
    type NodeType = Self;
}

impl ShaderGraphNode for MathNode {
    fn title(&self) -> &str {
        "Math"
    }

    fn title_color(&self) -> Color {
        Color::from_rgb8(200, 200, 100)
    }

    fn create_inputs(&self) -> Vec<ShaderGraphDefaultInputSlot> {
        vec![
            ShaderGraphDefaultInputSlot::new::<FloatType>("A", 0.0),
            ShaderGraphDefaultInputSlot::new::<FloatType>("B", 0.0),
            ShaderGraphDefaultInputSlot::unconnectable::<MathNodeMode>("Mode", MathNodeMode::Add),
        ]
    }

    fn create_outputs(&self) -> Vec<ShaderGraphDefaultOutputSlot> {
        vec![ShaderGraphDefaultOutputSlot::new::<FloatType>("Result")]
    }

    fn generate_code(&self, ctx: ShaderGraphNodeCodeGenContext) -> String {
        let a = ctx.get_input::<0>().unwrap();
        let b = ctx.get_input::<1>().unwrap();
        let mode = ctx.get_input_raw::<2, MathNodeMode>().unwrap();
        let output = ctx.get_output::<0>().unwrap();
        match mode {
            MathNodeMode::Add => format!("let {} = {} + {};", output, a, b),
            MathNodeMode::Subtract => format!("let {} = {} - {};", output, a, b),
            MathNodeMode::Multiply => format!("let {} = {} * {};", output, a, b),
            MathNodeMode::Divide => format!("let {} = {} / {};", output, a, b),
        }
    }
}
