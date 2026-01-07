use std::{
    any::Any,
    collections::{HashMap, VecDeque, hash_map::Entry},
    sync::Arc,
};

use cyancia_id::Id;
use dyn_clone::DynClone;
use iced_core::{Color, Element, Point};
use indexmap::IndexMap;

pub mod editor;
pub mod serde;

pub type GraphTheme = iced_core::Theme;
pub type GraphRenderer = iced_wgpu::Renderer;

#[derive(Debug, thiserror::Error)]
pub enum GraphCompileError {
    #[error("Invalid function signature")]
    InvalidFunctionSignature,
    #[error("{0}")]
    NodeCodeGenError(ContextualGraphNodeCodeGenError),
    #[error(transparent)]
    CustomError(anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GraphNodeCodeGenError {
    #[error("Input slot index out of bounds")]
    SlotIndexOutOfBounds,
    #[error("Missing input slot")]
    MissingInputSlot,
    #[error("Missing output slot")]
    MissingOutputSlot,
    #[error("Failed to cast variable")]
    FailedToCastVariable,
    #[error("Failed to convert literal to code")]
    LiteralToCodeFailed,
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct ContextualGraphNodeCodeGenError {
    pub node_id: Id<GraphNodeData>,
    pub node_title: String,
    pub err: GraphNodeCodeGenError,
    pub code: String,
}

impl std::fmt::Display for ContextualGraphNodeCodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error in node {:?} of type {}: {}\nCode already generated:\n{}",
            self.node_id, self.node_title, self.err, self.code
        )
    }
}

pub struct Graph {
    nodes: HashMap<Id<GraphNodeData>, GraphNodeData>,
    slots: GraphSlots,
    ident_generator: GraphVarIdentGenerator,
    signature: GraphFunctionSignature,
    cached_run_order: Option<Vec<Id<GraphNodeData>>>,
    storage: Arc<GraphDynamicInstancesStorage>,
}

impl Graph {
    pub fn new(
        signature: GraphFunctionSignature,
        storage: Arc<GraphDynamicInstancesStorage>,
    ) -> Self {
        Self {
            nodes: HashMap::new(),
            slots: GraphSlots::default(),
            ident_generator: GraphVarIdentGenerator::default(),
            signature,
            storage,
            cached_run_order: None,
        }
    }

    pub fn add_boxed_node(
        &mut self,
        position: Point,
        node: Box<dyn GraphNode>,
    ) -> Id<GraphNodeData> {
        let node_id = Id::random();
        let raw_inputs = node.create_inputs();
        let mut inputs = Vec::with_capacity(raw_inputs.len());
        for slot in raw_inputs {
            let slot_id = Id::random();
            self.slots.inputs.insert(
                slot_id,
                GraphInputSlotData {
                    node_id,
                    name: slot.name,
                    data: slot.value,
                    connected: None,
                    slot_type: slot.slot_type,
                },
            );
            inputs.push(slot_id);
        }

        let raw_outputs = node.create_outputs();
        let mut outputs = Vec::with_capacity(raw_outputs.len());
        for slot in raw_outputs {
            let slot_id = Id::random();
            self.slots.outputs.insert(
                slot_id,
                GraphOutputSlotData {
                    node_id,
                    name: slot.name,
                    data: GraphVariable {
                        identifier: self.ident_generator.next_output(),
                        ty: slot.ty,
                    },
                },
            );
            outputs.push(slot_id);
        }

        self.nodes.insert(
            node_id,
            GraphNodeData {
                position,
                inputs,
                outputs,
                data: node,
            },
        );
        self.invalidate_cache();
        node_id
    }

    pub fn add_node<T: GraphNode>(&mut self, position: Point, node: T) -> Id<GraphNodeData> {
        self.add_boxed_node(position, Box::new(node))
    }

    pub fn get_node(&self, id: &Id<GraphNodeData>) -> Option<&GraphNodeData> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &Id<GraphNodeData>) -> Option<&mut GraphNodeData> {
        self.nodes.get_mut(id)
    }

    pub fn connect_slots(&mut self, from: Id<GraphOutputSlotData>, to: Id<GraphInputSlotData>) {
        if !self.can_connect_slots(from, to) {
            return;
        }

        if let Some(input_slot) = self.slots.inputs.get_mut(&to) {
            input_slot.connected = Some(from);
            self.invalidate_cache();
        }
    }

    pub fn can_connect_slots(
        &self,
        from: Id<GraphOutputSlotData>,
        to: Id<GraphInputSlotData>,
    ) -> bool {
        let from_slot = self.slots.outputs.get(&from);
        let to_slot = self.slots.inputs.get(&to);

        if let (Some(from), Some(to)) = (from_slot, to_slot) {
            from.data.ty().name() == to.data.ty().name()
                || self.storage.casters.can_cast(from.data.ty(), to.data.ty())
        } else {
            false
        }
    }

    pub fn disconnect_slot(&mut self, to: Id<GraphInputSlotData>) {
        if let Some(input_slot) = self.slots.inputs.get_mut(&to) {
            input_slot.connected = None;
            self.invalidate_cache();
        }
    }

    pub fn connect_slots_by_index(
        &mut self,
        from_node: Id<GraphNodeData>,
        from_output_index: usize,
        to_node: Id<GraphNodeData>,
        to_input_index: usize,
    ) {
        let from_slot = self
            .nodes
            .get(&from_node)
            .and_then(|node| node.outputs.get(from_output_index))
            .cloned();
        let to_slot = self
            .nodes
            .get(&to_node)
            .and_then(|node| node.inputs.get(to_input_index))
            .cloned();

        if let (Some(from), Some(to)) = (from_slot, to_slot) {
            self.connect_slots(from, to);
            self.invalidate_cache();
        }
    }

    pub fn disconnect_slots_by_index(&mut self, to_node: Id<GraphNodeData>, to_input_index: usize) {
        let to_slot = self
            .nodes
            .get(&to_node)
            .and_then(|node| node.inputs.get(to_input_index))
            .cloned();

        if let Some(to) = to_slot {
            self.disconnect_slot(to);
            self.invalidate_cache();
        }
    }

    pub fn compile(&mut self) -> Result<String, GraphCompileError> {
        if self.cached_run_order.is_none() {
            self.update_cache();
        }

        let run_order = self.cached_run_order.as_ref().unwrap();
        let mut code = String::new();
        for node_id in run_order.clone() {
            let node = self.nodes.get(&node_id).unwrap();
            let context = GraphNodeCodeGenContext {
                inputs: &node.inputs,
                outputs: &node.outputs,
                graph_slots: &mut self.slots,
                casters: &self.storage.casters,
            };

            match node.data.generate_code(context) {
                Ok(node_code) => code.push_str(&node_code),
                Err(err) => {
                    return Err(GraphCompileError::NodeCodeGenError(
                        ContextualGraphNodeCodeGenError {
                            node_id,
                            node_title: node.data.name().to_string(),
                            err,
                            code: code.clone(),
                        },
                    ));
                }
            }
        }
        self.signature
            .compile(code)
            .ok_or(GraphCompileError::InvalidFunctionSignature)
    }

    pub fn invalidate_cache(&mut self) {
        self.cached_run_order = None;
    }

    pub fn update_cache(&mut self) {
        let mut out_degrees = self
            .nodes
            .iter()
            .map(|(node_id, node)| {
                (
                    *node_id,
                    node.outputs
                        .iter()
                        .map(|output_id| {
                            self.slots
                                .inputs
                                .iter()
                                .filter(|(_, slot)| slot.connected == Some(*output_id))
                                .count()
                        })
                        .sum::<usize>(),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut run_order = Vec::with_capacity(self.nodes.len());
        let mut ready_nodes = self
            .nodes
            .iter()
            .filter(|(_, node)| node.outputs.len() == 0)
            .map(|(node_id, _)| *node_id)
            .collect::<VecDeque<_>>();

        while let Some(node_id) = ready_nodes.pop_front() {
            run_order.push(node_id);
            let node = self.nodes.get(&node_id).unwrap();

            for input_slot_id in &node.inputs {
                let Some(from_node_id) = self
                    .slots
                    .get_connected(input_slot_id)
                    .map(|slot| slot.node_id)
                else {
                    continue;
                };

                // println!(
                //     "Visiting node {:?} from {:?} {}",
                //     node_id,
                //     from_node_id,
                //     out_degrees.get(&from_node_id).unwrap_or(&usize::MAX)
                // );
                let Entry::Occupied(out_degree_of_from_node) = out_degrees.entry(from_node_id)
                else {
                    continue;
                };

                if *out_degree_of_from_node.get() == 1 {
                    out_degree_of_from_node.remove();
                    ready_nodes.push_back(from_node_id);
                } else {
                    *out_degree_of_from_node.into_mut() -= 1;
                }
            }
        }

        run_order.reverse();
        self.cached_run_order = Some(dbg!(run_order));
    }

    pub fn update_literal(&mut self, message: ErasedGraphLiteralUpdateMessage) {
        if let Some(slot) = self.slots.inputs.get_mut(&message.id) {
            slot.data.ty.update_literal(&mut slot.data.value, message);
        }
    }

    pub fn storage(&self) -> &Arc<GraphDynamicInstancesStorage> {
        &self.storage
    }
}

#[derive(Default)]
pub struct GraphDynamicInstancesStorage {
    pub creators: GraphNodeCreatorStorage,
    pub types: GraphValueTypeStorage,
    pub casters: GraphTypeCastersStorage,
}

#[derive(Default)]
pub struct GraphSlots {
    inputs: HashMap<Id<GraphInputSlotData>, GraphInputSlotData>,
    outputs: HashMap<Id<GraphOutputSlotData>, GraphOutputSlotData>,
}

impl GraphSlots {
    pub fn get_input(&self, id: &Id<GraphInputSlotData>) -> Option<&GraphInputSlotData> {
        self.inputs.get(id)
    }

    pub fn get_output(&self, id: &Id<GraphOutputSlotData>) -> Option<&GraphOutputSlotData> {
        self.outputs.get(id)
    }

    pub fn get_connected(&self, input_id: &Id<GraphInputSlotData>) -> Option<&GraphOutputSlotData> {
        let input_node = self.inputs.get(input_id)?;
        let connected_id = input_node.connected.as_ref()?;
        self.outputs.get(connected_id)
    }
}

pub struct GraphFunctionSignature {
    name: String,
    ret_type: Box<dyn ErasedGraphValueType>,
    params: Vec<GraphVariable>,
}

impl GraphFunctionSignature {
    pub fn new<T: GraphValueType>(name: String, ret_type: T) -> Self {
        Self {
            name,
            ret_type: Box::new(ret_type),
            params: Vec::new(),
        }
    }

    pub fn with_param<T: GraphValueType + Default>(mut self, identifier: String) -> Self {
        self.params.push(GraphVariable::new::<T>(identifier));
        self
    }

    pub fn compile(&self, body: String) -> Option<String> {
        let ret_ty = self.ret_type.wgsl_type()?;

        let params = self
            .params
            .iter()
            .filter_map(|param| {
                param
                    .ty()
                    .wgsl_type()
                    .map(|ty| format!("{}: {}", param.identifier(), ty))
            })
            .collect::<Vec<_>>();
        if params.len() != self.params.len() {
            return None;
        }

        Some(format!(
            "fn {}({}) -> {} {{\n{}\n}}",
            self.name,
            params.join(", "),
            ret_ty,
            body
        ))
    }
}

pub trait GraphNodeCreator: 'static {
    type NodeType: GraphNode;
    fn create(&self) -> Self::NodeType;
}

pub trait ErasedGraphNodeCreator: 'static {
    fn create(&self) -> Box<dyn GraphNode>;
}

impl<T: GraphNodeCreator> ErasedGraphNodeCreator for T {
    fn create(&self) -> Box<dyn GraphNode> {
        Box::new(self.create())
    }
}

pub struct GraphNodeData {
    pub position: Point,
    pub data: Box<dyn GraphNode>,
    pub inputs: Vec<Id<GraphInputSlotData>>,
    pub outputs: Vec<Id<GraphOutputSlotData>>,
}

pub trait GraphNode: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn header_color(&self) -> Color;
    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot>;
    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError>;
}

pub struct GraphNodeCodeGenContext<'a> {
    pub inputs: &'a [Id<GraphInputSlotData>],
    pub outputs: &'a [Id<GraphOutputSlotData>],
    pub graph_slots: &'a mut GraphSlots,
    pub casters: &'a GraphTypeCastersStorage,
}

impl GraphNodeCodeGenContext<'_> {
    pub fn get_input<const N: usize>(&self) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(N)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        let Some(connected) = slot.connected else {
            // Literal value should always has the same type as the slot type.
            return slot
                .data
                .to_code()
                .ok_or(GraphNodeCodeGenError::LiteralToCodeFailed);
        };

        let output_slot = self
            .graph_slots
            .get_output(&connected)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        if output_slot.data.ty().name() != slot.data.ty().name() {
            self.casters
                .try_cast(&output_slot.data, slot.data.ty())
                .ok_or(GraphNodeCodeGenError::FailedToCastVariable)
        } else {
            Ok(output_slot.data.identifier.clone())
        }
    }

    pub fn get_input_raw<const N: usize, T: 'static>(&self) -> Result<&T, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(N)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        Ok(slot.data.as_ref::<T>())
    }

    pub fn get_output<const N: usize>(&self) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .outputs
            .get(N)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_output(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        Ok(slot.data.identifier.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphSlotType {
    Normal,
    Unconnectable,
    Hidden,
}

pub struct GraphDefaultInputSlot {
    pub name: &'static str,
    pub value: GraphLiteral,
    pub slot_type: GraphSlotType,
}

impl GraphDefaultInputSlot {
    pub fn new<T: GraphValueType + Default>(
        name: &'static str,
        value: T::AssociatedLiteralType,
    ) -> Self {
        Self {
            name,
            value: GraphLiteral::new::<T>(value),
            slot_type: GraphSlotType::Normal,
        }
    }

    pub fn unconnectable<T: GraphValueType + Default>(
        name: &'static str,
        value: T::AssociatedLiteralType,
    ) -> Self {
        Self {
            name,
            value: GraphLiteral::new::<T>(value),
            slot_type: GraphSlotType::Unconnectable,
        }
    }

    pub fn hidden<T: GraphValueType + Default>(value: T::AssociatedLiteralType) -> Self {
        Self {
            name: Default::default(),
            value: GraphLiteral::new::<T>(value),
            slot_type: GraphSlotType::Hidden,
        }
    }

    pub fn new_non_default<T: GraphValueType>(
        name: &'static str,
        value: T::AssociatedLiteralType,
        ty: T,
        slot_type: GraphSlotType,
    ) -> Self {
        Self {
            name,
            value: GraphLiteral::new_non_default::<T>(value, ty),
            slot_type,
        }
    }
}

pub struct GraphInputSlotData {
    pub node_id: Id<GraphNodeData>,
    pub name: &'static str,
    pub data: GraphLiteral,
    pub connected: Option<Id<GraphOutputSlotData>>,
    pub slot_type: GraphSlotType,
}

pub struct GraphDefaultOutputSlot {
    pub name: &'static str,
    pub ty: Box<dyn ErasedGraphValueType>,
}

impl GraphDefaultOutputSlot {
    pub fn new<T: GraphValueType + Default>(name: &'static str) -> Self {
        Self {
            name,
            ty: Box::new(T::default()),
        }
    }
}

pub struct GraphOutputSlotData {
    pub node_id: Id<GraphNodeData>,
    pub name: &'static str,
    pub data: GraphVariable,
}

pub trait GraphValueType: Send + Sync + 'static + DynClone {
    type AssociatedLiteralType: Send + Sync + 'static;
    type Message: Send + Sync + 'static;
    fn color(&self) -> Color;
    fn name(&self) -> &'static str;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer>;
    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message);
    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String>;
}

#[derive(Debug)]
pub struct ErasedGraphLiteralUpdateMessage {
    pub inner: Box<dyn Any + Send + Sync>,
    pub id: Id<GraphInputSlotData>,
}

pub trait ErasedGraphValueType: Send + Sync + 'static + DynClone {
    fn color(&self) -> Color;
    fn name(&self) -> &'static str;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn view_literal(
        &self,
        slot_id: Id<GraphInputSlotData>,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer>;
    fn update_literal(
        &self,
        data: &mut Box<dyn Any + Send + Sync>,
        message: ErasedGraphLiteralUpdateMessage,
    );
    fn literal_to_code(&self, data: &Box<dyn Any + Send + Sync>) -> Option<String>;
}

impl<T: GraphValueType> ErasedGraphValueType for T {
    fn color(&self) -> Color {
        self.color()
    }

    fn name(&self) -> &'static str {
        self.name()
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        self.wgsl_type()
    }

    fn view_literal(
        &self,
        slot_id: Id<GraphInputSlotData>,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer> {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.view_literal(literal)
            .map(move |msg| ErasedGraphLiteralUpdateMessage {
                inner: Box::new(msg),
                id: slot_id,
            })
    }

    fn update_literal(
        &self,
        data: &mut Box<dyn Any + Send + Sync>,
        message: ErasedGraphLiteralUpdateMessage,
    ) {
        let literal = data
            .downcast_mut::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        let msg = message
            .inner
            .downcast::<T::Message>()
            .expect("Failed to downcast update message.");
        self.update_literal(literal, *msg);
    }

    fn literal_to_code(&self, data: &Box<dyn Any + Send + Sync>) -> Option<String> {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.literal_to_code(literal)
    }
}

pub struct GraphLiteral {
    value: Box<dyn Any + Send + Sync>,
    ty: Box<dyn ErasedGraphValueType>,
}

impl GraphLiteral {
    pub fn new<T: GraphValueType + Default>(value: T::AssociatedLiteralType) -> Self {
        Self {
            value: Box::new(value),
            ty: Box::new(T::default()),
        }
    }

    pub fn new_non_default<T: GraphValueType>(value: T::AssociatedLiteralType, ty: T) -> Self {
        Self {
            value: Box::new(value),
            ty: Box::new(ty),
        }
    }

    pub fn as_ref<T: 'static>(&self) -> &T {
        self.value
            .downcast_ref::<T>()
            .expect("Failed to downcast Literal")
    }

    pub fn as_mut<T: 'static>(&mut self) -> &mut T {
        self.value
            .downcast_mut::<T>()
            .expect("Failed to downcast Literal")
    }

    pub fn ty(&self) -> &dyn ErasedGraphValueType {
        self.ty.as_ref()
    }

    pub fn set<T: 'static>(&mut self, value: T) {
        if let Some(x) = self.value.downcast_mut() {
            *x = value;
        } else {
            log::error!("Setting a Literal with a different type");
        }
    }

    pub fn to_code(&self) -> Option<String> {
        self.ty.literal_to_code(&self.value)
    }
}

pub struct GraphVariable {
    identifier: String,
    ty: Box<dyn ErasedGraphValueType>,
}

impl GraphVariable {
    pub fn new<T: GraphValueType + Default>(identifier: String) -> Self {
        Self {
            identifier,
            ty: Box::new(T::default()),
        }
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn ty(&self) -> &dyn ErasedGraphValueType {
        self.ty.as_ref()
    }
}

#[derive(Default)]
pub struct GraphNodeCreatorStorage {
    creators: IndexMap<&'static str, Box<dyn ErasedGraphNodeCreator>>,
}

impl GraphNodeCreatorStorage {
    pub fn register<T: GraphNodeCreator + Default>(&mut self) {
        let node = T::default().create();
        let creator = Box::new(T::default());
        self.creators.insert(node.name(), creator);
    }

    pub fn register_non_default<T: GraphNodeCreator>(&mut self, creator: T) {
        let node = creator.create();
        let creator = Box::new(creator);
        self.creators.insert(node.name(), creator);
    }

    pub fn get(&self, name: &str) -> Option<&Box<dyn ErasedGraphNodeCreator>> {
        self.creators.get(name)
    }

    pub fn all(&self) -> &IndexMap<&'static str, Box<dyn ErasedGraphNodeCreator>> {
        &self.creators
    }
}

#[derive(Default)]
pub struct GraphValueTypeStorage {
    types: HashMap<&'static str, Box<dyn ErasedGraphValueType>>,
}

impl GraphValueTypeStorage {
    pub fn register<T: GraphValueType + Default>(&mut self) {
        let ty = T::default();
        self.types.insert(ty.name(), Box::new(ty));
    }

    pub fn register_non_default<T: GraphValueType>(&mut self, ty: T) {
        self.types.insert(ty.name(), Box::new(ty));
    }

    pub fn get(&self, name: &str) -> Option<&Box<dyn ErasedGraphValueType>> {
        self.types.get(name)
    }

    pub fn all(&self) -> &HashMap<&'static str, Box<dyn ErasedGraphValueType>> {
        &self.types
    }
}

#[derive(Default)]
pub struct GraphTypeCastersStorage {
    casters: HashMap<&'static str, HashMap<&'static str, Box<dyn ErasedGraphVariableCaster>>>,
}

impl GraphTypeCastersStorage {
    pub fn register<T: GraphVariableCaster + Default>(&mut self) {
        let from = T::FromType::default();
        let to = T::ToType::default();
        let from_name = <T::FromType as GraphValueType>::name(&from);
        let to_name = <T::ToType as GraphValueType>::name(&to);
        let caster: Box<dyn ErasedGraphVariableCaster> = Box::new(T::default());
        self.casters
            .entry(from_name)
            .or_default()
            .insert(to_name, caster);
    }

    pub fn try_cast(
        &self,
        variable: &GraphVariable,
        to_type: &dyn ErasedGraphValueType,
    ) -> Option<String> {
        let from_name = variable.ty().name();
        let to_name = to_type.name();
        let caster = self.casters.get(from_name)?.get(to_name)?;
        Some(caster.cast(&variable.identifier))
    }

    pub fn can_cast(&self, from: &dyn ErasedGraphValueType, to: &dyn ErasedGraphValueType) -> bool {
        let from_name = from.name();
        let to_name = to.name();
        self.casters
            .get(from_name)
            .and_then(|map| map.get(to_name))
            .is_some()
    }

    pub fn all(
        &self,
    ) -> &HashMap<&'static str, HashMap<&'static str, Box<dyn ErasedGraphVariableCaster>>> {
        &self.casters
    }
}

pub trait GraphVariableCaster: 'static {
    type FromType: GraphValueType + Default;
    type ToType: GraphValueType + Default;
    fn cast(&self, variable: &String) -> String;
}

pub trait ErasedGraphVariableCaster {
    fn cast(&self, variable: &String) -> String;
}

impl<T: GraphVariableCaster> ErasedGraphVariableCaster for T {
    fn cast(&self, variable: &String) -> String {
        self.cast(variable)
    }
}

#[derive(Default)]
pub struct GraphVarIdentGenerator {
    counter: usize,
}

impl GraphVarIdentGenerator {
    pub fn next_output(&mut self) -> String {
        let ident = format!("output_{}", self.counter);
        self.counter += 1;
        ident
    }
}
