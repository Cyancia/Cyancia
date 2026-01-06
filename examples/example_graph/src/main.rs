use std::{any::TypeId, collections::HashMap, fmt::Display, marker::PhantomData, sync::Arc};

use cyancia_id::{Id, UntypedId};
use cyancia_shader_graph::{
    ErasedShaderGraphNodeCreator, ShaderGraph, ShaderGraphCompileError,
    ShaderGraphDefaultInputSlot, ShaderGraphDefaultOutputSlot, ShaderGraphFunctionSignature,
    ShaderGraphNode, ShaderGraphNodeCodeGenContext, ShaderGraphNodeCreator, ShaderGraphRenderer,
    ShaderGraphSlotType, ShaderGraphTheme, ShaderGraphValueType, ShaderLiteral, ShaderVariable,
    ShaderVariableCaster,
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
use parking_lot::RwLock;

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
        let mut graph = ShaderGraph::new(
            ShaderGraphFunctionSignature::new("test".into(), FloatType)
                .with_param::<FloatType>("input1".into()),
        );
        let add1 = graph.add_node(Point::new(0.0, 0.0), AddNode);
        let add2 = graph.add_node(Point::new(200.0, 0.0), AddNode);
        graph.connect_slots_by_index(add1, 0, add2, 0);
        graph.add_caster::<Vector2DToFloatCaster>();
        let storage = Arc::new(ExternalDataStorage::default());
        storage.insert::<FloatType>(ExternalLiteral {
            name: "MyFloat".to_string(),
            value: ShaderLiteral::new::<FloatType>(0.5),
        });

        Self {
            graph,
            creators: vec![
                Box::new(AddNode),
                Box::new(Vector2DAddNode),
                Box::new(MathNode),
                Box::new(ExternalNodeCreator::<FloatType> {
                    storage: storage.clone(),
                    marker: PhantomData,
                }),
                Box::new(DummyOutputNode),
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

        match self.graph.compile() {
            Ok(code) => println!("{}", code),
            Err(e) => println!("Code generation failed: {}", e),
        }
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

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("f32")
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

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("{:.5}", data))
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

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("vec2f")
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

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("vec2f({:.2}, {:.2})", data.x, data.y))
    }
}

#[derive(Default)]
pub struct AddNode;

impl ShaderGraphNodeCreator for AddNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        AddNode
    }
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

    fn generate_code(
        &self,
        ctx: ShaderGraphNodeCodeGenContext,
    ) -> Result<String, ShaderGraphCompileError> {
        let a = ctx.get_input::<0>()?;
        let b = ctx.get_input::<1>()?;
        let output = ctx.get_output::<0>()?;
        Ok(format!("let {} = {} + {};\n", output, a, b))
    }
}

#[derive(Default)]
pub struct Vector2DAddNode;

impl ShaderGraphNodeCreator for Vector2DAddNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        Vector2DAddNode
    }
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

    fn generate_code(
        &self,
        ctx: ShaderGraphNodeCodeGenContext,
    ) -> Result<String, ShaderGraphCompileError> {
        let a = ctx.get_input::<0>()?;
        let b = ctx.get_input::<1>()?;
        let output = ctx.get_output::<0>()?;
        Ok(format!("let {} = {} + {};\n", output, a, b))
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

    fn wgsl_type(&self) -> Option<&'static str> {
        None
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

    fn literal_to_code(&self, _data: &Self::AssociatedLiteralType) -> Option<String> {
        None
    }
}

impl ShaderGraphNodeCreator for MathNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        MathNode
    }
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

    fn generate_code(
        &self,
        ctx: ShaderGraphNodeCodeGenContext,
    ) -> Result<String, ShaderGraphCompileError> {
        let a = ctx.get_input::<0>()?;
        let b = ctx.get_input::<1>()?;
        let mode = ctx.get_input_raw::<2, MathNodeMode>()?;
        let output = ctx.get_output::<0>()?;
        Ok(match mode {
            MathNodeMode::Add => format!("let {} = {} + {};\n", output, a, b),
            MathNodeMode::Subtract => format!("let {} = {} - {};\n", output, a, b),
            MathNodeMode::Multiply => format!("let {} = {} * {};\n", output, a, b),
            MathNodeMode::Divide => format!("let {} = {} / {};\n", output, a, b),
        })
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ExternalLiteralId {
    pub name: String,
}

impl ToString for ExternalLiteralId {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

pub struct ExternalLiteral {
    pub name: String,
    pub value: ShaderLiteral,
}

#[derive(Default)]
pub struct ExternalDataStorage {
    contents: RwLock<HashMap<ExternalLiteralId, Arc<ExternalLiteral>>>,
    types: RwLock<HashMap<TypeId, Vec<ExternalLiteralId>>>,
}

impl ExternalDataStorage {
    pub fn insert<T: ShaderGraphValueType>(&self, value: ExternalLiteral) {
        let mut contents = self.contents.write();
        let mut types = self.types.write();
        let id = ExternalLiteralId {
            name: value.name.clone(),
        };

        contents.insert(id.clone(), Arc::new(value));
        types.entry(TypeId::of::<T>()).or_default().push(id);
    }

    pub fn get<T: ShaderGraphValueType>(
        &self,
        id: &ExternalLiteralId,
    ) -> Option<Arc<ExternalLiteral>> {
        self.contents.read().get(&id).cloned()
    }

    pub fn all_of_type<T: ShaderGraphValueType>(&self) -> Vec<ExternalLiteralId> {
        self.types
            .read()
            .get(&TypeId::of::<T>())
            .cloned()
            .unwrap_or_default()
    }
}

pub struct ExternalLiteralValue<T> {
    id: Option<ExternalLiteralId>,
    marker: PhantomData<T>,
}

impl<T> Clone for ExternalLiteralValue<T> {
    fn clone(&self) -> Self {
        ExternalLiteralValue {
            id: self.id.clone(),
            marker: PhantomData,
        }
    }
}

pub struct ExternalLiteralType<T> {
    storage: Arc<ExternalDataStorage>,
    marker: PhantomData<T>,
}

impl<T: ShaderGraphValueType> ShaderGraphValueType for ExternalLiteralType<T> {
    type AssociatedLiteralType = ExternalLiteralValue<T>;

    type Message = ExternalLiteralId;

    fn color(&self) -> Color {
        Color::from_rgb8(200, 100, 200)
    }

    fn name(&self) -> &'static str {
        "External Data"
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        let literal = self
            .storage
            .get::<T>(&self.storage.all_of_type::<T>().get(0)?.clone())?;
        literal.value.ty().wgsl_type()
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, ShaderGraphTheme, ShaderGraphRenderer> {
        let options = self.storage.all_of_type::<T>();
        pick_list(options, data.id.clone(), |msg| msg).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = ExternalLiteralValue {
            id: Some(message),
            marker: PhantomData,
        };
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        let id = data.id.as_ref()?;
        let literal = self.storage.get::<T>(id)?;
        literal.value.to_code()
    }
}

pub struct ExternalNodeCreator<T> {
    pub storage: Arc<ExternalDataStorage>,
    pub marker: PhantomData<T>,
}

pub struct ExternalNode<T> {
    storage: Arc<ExternalDataStorage>,
    marker: PhantomData<T>,
}

impl<T: ShaderGraphValueType + Default> ShaderGraphNodeCreator for ExternalNodeCreator<T> {
    type NodeType = ExternalNode<T>;

    fn create(&self) -> Self::NodeType {
        ExternalNode {
            storage: self.storage.clone(),
            marker: PhantomData,
        }
    }
}

impl<T: ShaderGraphValueType + Default> ShaderGraphNode for ExternalNode<T> {
    fn title(&self) -> &str {
        "External"
    }

    fn title_color(&self) -> Color {
        Color::from_rgb8(150, 150, 250)
    }

    fn create_inputs(&self) -> Vec<ShaderGraphDefaultInputSlot> {
        vec![ShaderGraphDefaultInputSlot::new_non_default::<
            ExternalLiteralType<T>,
        >(
            "Id",
            ExternalLiteralValue {
                id: None,
                marker: PhantomData,
            },
            ExternalLiteralType {
                storage: self.storage.clone(),
                marker: PhantomData,
            },
            ShaderGraphSlotType::Unconnectable,
        )]
    }

    fn create_outputs(&self) -> Vec<ShaderGraphDefaultOutputSlot> {
        vec![ShaderGraphDefaultOutputSlot::new::<FloatType>("Value")]
    }

    fn generate_code(
        &self,
        ctx: ShaderGraphNodeCodeGenContext,
    ) -> Result<String, ShaderGraphCompileError> {
        let input = ctx.get_input::<0>()?;
        let output = ctx.get_output::<0>()?;
        Ok(format!("let {} = {};\n", output, input))
    }
}

#[derive(Default)]
pub struct DummyOutputNode;

impl ShaderGraphNodeCreator for DummyOutputNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        DummyOutputNode
    }
}

impl ShaderGraphNode for DummyOutputNode {
    fn title(&self) -> &str {
        "Output"
    }

    fn title_color(&self) -> Color {
        Color::from_rgb8(100, 200, 200)
    }

    fn create_inputs(&self) -> Vec<ShaderGraphDefaultInputSlot> {
        vec![ShaderGraphDefaultInputSlot::new::<FloatType>("Input", 0.0)]
    }

    fn create_outputs(&self) -> Vec<ShaderGraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        ctx: ShaderGraphNodeCodeGenContext,
    ) -> Result<String, ShaderGraphCompileError> {
        let input = ctx.get_input::<0>()?;
        Ok(format!("return {};", input))
    }
}
