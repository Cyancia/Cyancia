use std::{any::TypeId, collections::HashMap, fmt::Display, marker::PhantomData, sync::Arc};

use anyhow::anyhow;
use cyancia_id::{Id, UntypedId};
use cyancia_shader_graph::{
    ErasedGraphLiteralUpdateMessage, ErasedGraphNodeCreator, Graph, GraphCompileError,
    GraphDefaultInputSlot, GraphDefaultOutputSlot, GraphDeserializer, GraphDynamicInstancesStorage,
    GraphFunctionSignature, GraphLiteral, GraphNode, GraphNodeCodeGenContext,
    GraphNodeCodeGenError, GraphNodeCreator, GraphNodeUpdateContext, GraphNodeViewContext,
    GraphRenderer, GraphSerializer, GraphSlotType, GraphTheme, GraphValueType, GraphVariable,
    GraphVariableCaster, StatelessCommonGraphNode,
    editor::{GraphView, GraphViewMessage},
    save::SerializableGraph,
};
use cyancia_utils::wrapper;
use cyancia_widgets::{drag_field::DragField, spin_slider::SpinSlider};
use glam::Vec2;
use iced::{
    Color, Element,
    Length::Fill,
    Point, Renderer, Subscription, Theme,
    advanced::{Widget, layout},
    keyboard::{self, key},
    widget::{Text, column, combo_box, container, pick_list, row, sensor},
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

fn main() {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .run()
        .unwrap();
}

pub struct App {
    graph: Graph,
}

#[derive(Debug)]
pub enum GraphMessage {
    Keyboard(keyboard::Event),
    View(GraphViewMessage),
}

impl App {
    pub fn new() -> Self {
        let external_data = Arc::new(ExternalDataStorage::default());
        external_data.insert::<FloatType>(ExternalLiteral {
            name: "MyFloat".to_string(),
            value: GraphLiteral::new::<FloatType>(0.5),
        });

        let mut storage = GraphDynamicInstancesStorage::default();
        storage.types.register::<FloatType>();
        storage.types.register::<Vector2DType>();
        storage.types.register::<MathNodeMode>();
        storage
            .types
            .register_non_default(ExternalLiteralType::<FloatType> {
                storage: external_data.clone(),
                marker: PhantomData,
            });
        storage.creators.register::<AddNode>();
        storage.creators.register::<Vector2DAddNode>();
        storage.creators.register::<MathNode>();
        storage
            .creators
            .register_non_default(ExternalNodeCreator::<FloatType> {
                storage: external_data.clone(),
                marker: PhantomData,
            });
        storage.creators.register::<DummyOutputNode>();
        storage.casters.register::<Vector2DToFloatCaster>();
        let storage = Arc::new(storage);

        let mut graph = Graph::new(
            GraphFunctionSignature::new("test".into(), FloatType)
                .with_param::<FloatType>("input1".into()),
            storage,
        );
        let add1 = graph.add_node(Point::new(0.0, 0.0), AddNode);
        let add2 = graph.add_node(Point::new(200.0, 0.0), AddNode);
        graph.connect_slots_by_index(add1, 0, add2, 0);

        Self {
            graph,
            // creators: vec![Box::new(AddNodeCreator)],
            // viewers,
        }
    }

    // pub fn view(&self) -> Element<'_, GraphEditorMessage<GraphMessage>> {
    pub fn view(&self) -> Element<'_, GraphMessage, GraphTheme, GraphRenderer> {
        row![
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
                GraphViewMessage::NodeDeleteRequest(id) => {
                    self.graph.delete_node(&id);
                }
                GraphViewMessage::NodeCreateRequest(point, node) => {
                    self.graph.add_boxed_node(point, node);
                }
                GraphViewMessage::NodeUpdate(message) => {
                    self.graph.update_node(message);
                }
            },
            GraphMessage::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                location,
                modifiers,
                text,
                repeat,
            }) => {
                if repeat {
                    return;
                }

                if modifiers.control() {
                    if physical_key == key::Physical::Code(key::Code::KeyS) {
                        std::fs::write("test.toml", self.graph.to_toml().unwrap()).unwrap();
                    }
                    if physical_key == key::Physical::Code(key::Code::KeyL) {
                        let s = std::fs::read_to_string("test.toml").unwrap();
                        let (graph, errors) = Graph::from_toml(self.graph.storage().clone(), &s);
                        for error in errors {
                            println!("Deserialization error: {}", error);
                        }
                        if let Some(graph) = graph {
                            self.graph = graph;
                        }
                    }
                }
            }
            _ => {}
        }

        match self.graph.compile() {
            Ok(code) => println!("{}", code),
            Err(e) => println!("Code generation failed: {}", e),
        }
    }

    pub fn subscription(&self) -> Subscription<GraphMessage> {
        keyboard::listen().map(GraphMessage::Keyboard)
    }
}

#[derive(Default, Clone)]
pub struct FloatType;

impl GraphValueType for FloatType {
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
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        SpinSlider::new(0.0..=1.0, *data, |x| x).step(0.01).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("{:.5}", data))
    }
}

#[derive(Default, Clone)]
pub struct Vector2DType;

#[derive(Clone)]
pub enum Vector2DTypeMessage {
    XChanged(f32),
    YChanged(f32),
}

impl GraphValueType for Vector2DType {
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
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
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

impl GraphNodeCreator for AddNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        AddNode
    }
}

impl StatelessCommonGraphNode for AddNode {
    fn name(&self) -> &'static str {
        "Add"
    }

    fn header_color(&self) -> Color {
        Color::from_rgb8(200, 100, 100)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<FloatType>("A", 0.0),
            GraphDefaultInputSlot::new::<FloatType>("B", 0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<FloatType>("Result")]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let a = ctx.get_input::<0>()?;
        let b = ctx.get_input::<1>()?;
        let output = ctx.get_output::<0>()?;
        Ok(format!("let {} = {} + {};\n", output, a, b))
    }
}

#[derive(Default)]
pub struct Vector2DAddNode;

impl GraphNodeCreator for Vector2DAddNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        Vector2DAddNode
    }
}

impl StatelessCommonGraphNode for Vector2DAddNode {
    fn name(&self) -> &'static str {
        "Vector2D Add"
    }

    fn header_color(&self) -> Color {
        Color::from_rgb8(100, 100, 200)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<Vector2DType>("A", Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vector2DType>("B", Vec2::ZERO),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vector2DType>("Result")]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let a = ctx.get_input::<0>()?;
        let b = ctx.get_input::<1>()?;
        let output = ctx.get_output::<0>()?;
        Ok(format!("let {} = {} + {};\n", output, a, b))
    }
}

#[derive(Default, Clone)]
pub struct Vector2DToFloatCaster;

impl GraphVariableCaster for Vector2DToFloatCaster {
    type FromType = Vector2DType;

    type ToType = FloatType;

    fn cast(&self, variable: &String) -> String {
        format!("{}.x", variable)
    }
}

#[derive(Default)]
pub struct MathNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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

impl GraphValueType for MathNodeMode {
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
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
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

impl GraphNodeCreator for MathNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        MathNode
    }
}

#[derive(Clone)]
pub enum MathNodeMessage {
    ModeChanged(MathNodeMode),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl GraphNode for MathNode {
    type State = MathNodeMode;

    type Message = MathNodeMessage;

    fn name(&self) -> &'static str {
        "Math"
    }

    fn header_color(&self) -> Color {
        Color::from_rgb8(200, 200, 100)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<FloatType>("A", 0.0),
            GraphDefaultInputSlot::new::<FloatType>("B", 0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<FloatType>("Result")]
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let a = ctx.get_input::<0>()?;
        let b = ctx.get_input::<1>()?;
        let output = ctx.get_output::<0>()?;
        Ok(match state {
            MathNodeMode::Add => format!("let {} = {} + {};\n", output, a, b),
            MathNodeMode::Subtract => format!("let {} = {} - {};\n", output, a, b),
            MathNodeMode::Multiply => format!("let {} = {} * {};\n", output, a, b),
            MathNodeMode::Divide => format!("let {} = {} / {};\n", output, a, b),
        })
    }

    fn default_state(&self) -> Self::State {
        MathNodeMode::Add
    }

    fn view_body(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = column![];

        column = column.push(pick_list(
            vec![
                MathNodeMode::Add,
                MathNodeMode::Subtract,
                MathNodeMode::Multiply,
                MathNodeMode::Divide,
            ],
            Some(*state),
            |mode| MathNodeMessage::ModeChanged(mode),
        ));

        column
            .extend(
                ctx.view_all_inputs()
                    .into_iter()
                    .map(|e| e.map(|m| MathNodeMessage::LiteralUpdate(m))),
            )
            .into()
    }

    fn update_body(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            MathNodeMessage::ModeChanged(mode) => *state = mode,
            MathNodeMessage::LiteralUpdate(m) => {
                ctx.update_literal(m);
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    pub value: GraphLiteral,
}

#[derive(Default)]
pub struct ExternalDataStorage {
    contents: RwLock<HashMap<ExternalLiteralId, Arc<ExternalLiteral>>>,
    types: RwLock<HashMap<TypeId, Vec<ExternalLiteralId>>>,
}

impl ExternalDataStorage {
    pub fn insert<T: GraphValueType>(&self, value: ExternalLiteral) {
        let mut contents = self.contents.write();
        let mut types = self.types.write();
        let id = ExternalLiteralId {
            name: value.name.clone(),
        };

        contents.insert(id.clone(), Arc::new(value));
        types.entry(TypeId::of::<T>()).or_default().push(id);
    }

    pub fn get<T: GraphValueType>(&self, id: &ExternalLiteralId) -> Option<Arc<ExternalLiteral>> {
        self.contents.read().get(&id).cloned()
    }

    pub fn all_of_type<T: GraphValueType>(&self) -> Vec<ExternalLiteralId> {
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

impl<T> Serialize for ExternalLiteralValue<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.id.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for ExternalLiteralValue<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = Option::<ExternalLiteralId>::deserialize(deserializer)?;
        Ok(ExternalLiteralValue {
            id,
            marker: PhantomData,
        })
    }
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

impl<T> Clone for ExternalLiteralType<T> {
    fn clone(&self) -> Self {
        ExternalLiteralType {
            storage: self.storage.clone(),
            marker: PhantomData,
        }
    }
}

impl<T: GraphValueType> GraphValueType for ExternalLiteralType<T> {
    type AssociatedLiteralType = ExternalLiteralValue<T>;

    type Message = ExternalLiteralId;

    fn color(&self) -> Color {
        Color::from_rgb8(200, 100, 200)
    }

    fn name(&self) -> &'static str {
        // TODO make this constant.
        Box::leak(format!("External {}", std::any::type_name::<T>()).into_boxed_str())
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
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
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

impl<T: GraphValueType + Default> GraphNodeCreator for ExternalNodeCreator<T> {
    type NodeType = ExternalNode<T>;

    fn create(&self) -> Self::NodeType {
        ExternalNode {
            storage: self.storage.clone(),
            marker: PhantomData,
        }
    }
}

#[derive(Clone)]
pub enum ExternalNodeMessage {
    IdChanged(ExternalLiteralId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<T: GraphValueType + Default> GraphNode for ExternalNode<T> {
    type State = ExternalLiteralValue<T>;

    type Message = ExternalNodeMessage;

    fn name(&self) -> &'static str {
        "External"
    }

    fn header_color(&self) -> Color {
        Color::from_rgb8(150, 150, 250)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<FloatType>("Value")]
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let id = state
            .id
            .as_ref()
            .ok_or(anyhow!("No external literal selected"))?;
        let literal = self
            .storage
            .get::<T>(id)
            .ok_or(anyhow!("External literal not found"))?;
        let code = literal
            .value
            .to_code()
            .ok_or(anyhow!("Cannot convert literal to code"))?;
        let output = ctx.get_output::<0>()?;
        Ok(format!("let {} = {};\n", output, code))
    }

    fn default_state(&self) -> Self::State {
        ExternalLiteralValue {
            id: None,
            marker: PhantomData,
        }
    }

    fn view_body(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = column![];

        column = column.push(pick_list(
            self.storage.all_of_type::<T>(),
            state.id.clone(),
            |id| ExternalNodeMessage::IdChanged(id),
        ));

        column
            .extend(
                ctx.view_all_inputs()
                    .into_iter()
                    .map(|e| e.map(|m| ExternalNodeMessage::LiteralUpdate(m))),
            )
            .into()
    }

    fn update_body(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            ExternalNodeMessage::IdChanged(id) => state.id = Some(id),
            ExternalNodeMessage::LiteralUpdate(m) => {
                ctx.update_literal(m);
            }
        }
    }
}

#[derive(Default)]
pub struct DummyOutputNode;

impl GraphNodeCreator for DummyOutputNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        DummyOutputNode
    }
}

impl StatelessCommonGraphNode for DummyOutputNode {
    fn name(&self) -> &'static str {
        "Output"
    }

    fn header_color(&self) -> Color {
        Color::from_rgb8(100, 200, 200)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<FloatType>("Input", 0.0)]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let input = ctx.get_input::<0>()?;
        Ok(format!("return {};", input))
    }
}
