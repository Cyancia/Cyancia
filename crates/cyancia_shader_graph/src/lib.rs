use std::{
    any::Any,
    collections::{HashMap, VecDeque, hash_map::Entry},
    io::Write,
};

use cyancia_id::Id;
use cyancia_utils::wrapper;
use iced_core::{Color, Element, Point};

pub mod editor;

pub type ShaderGraphTheme = iced_core::Theme;
pub type ShaderGraphRenderer = iced_wgpu::Renderer;

#[derive(Debug, thiserror::Error)]
pub enum ShaderGraphCompileError {
    #[error("Invalid function signature")]
    InvalidFunctionSignature,
    #[error("{0}")]
    NodeCodeGenError(ContextualShaderGraphNodeCodeGenError),
    #[error(transparent)]
    CustomError(anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ShaderGraphNodeCodeGenError {
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
pub struct ContextualShaderGraphNodeCodeGenError {
    pub node_id: Id<ShaderGraphNodeData>,
    pub node_title: String,
    pub err: ShaderGraphNodeCodeGenError,
    pub code: String,
}

impl std::fmt::Display for ContextualShaderGraphNodeCodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error in node {:?} of type {}: {}\nCode already generated:\n{}",
            self.node_id, self.node_title, self.err, self.code
        )
    }
}

pub struct ShaderGraph {
    nodes: HashMap<Id<ShaderGraphNodeData>, ShaderGraphNodeData>,
    slots: ShaderGraphSlots,
    casters: ShaderGraphCasters,
    ident_generator: ShaderGraphIdentifierGenerator,
    signature: ShaderGraphFunctionSignature,
    cached_run_order: Option<Vec<Id<ShaderGraphNodeData>>>,
}

impl ShaderGraph {
    pub fn new(signature: ShaderGraphFunctionSignature) -> Self {
        Self {
            nodes: HashMap::new(),
            slots: ShaderGraphSlots::default(),
            casters: ShaderGraphCasters::default(),
            ident_generator: ShaderGraphIdentifierGenerator::default(),
            signature,
            cached_run_order: None,
        }
    }

    pub fn add_boxed_node(
        &mut self,
        position: Point,
        node: Box<dyn ShaderGraphNode>,
    ) -> Id<ShaderGraphNodeData> {
        let node_id = Id::random();
        let raw_inputs = node.create_inputs();
        let mut inputs = Vec::with_capacity(raw_inputs.len());
        for slot in raw_inputs {
            let slot_id = Id::random();
            self.slots.inputs.insert(
                slot_id,
                ShaderGraphInputSlotData {
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
                ShaderGraphOutputSlotData {
                    node_id,
                    name: slot.name,
                    data: ShaderVariable {
                        identifier: self.ident_generator.next_output(),
                        ty: slot.ty,
                    },
                },
            );
            outputs.push(slot_id);
        }

        self.nodes.insert(
            node_id,
            ShaderGraphNodeData {
                position,
                inputs,
                outputs,
                data: node,
            },
        );
        self.invalidate_cache();
        node_id
    }

    pub fn add_node<T: ShaderGraphNode>(
        &mut self,
        position: Point,
        node: T,
    ) -> Id<ShaderGraphNodeData> {
        self.add_boxed_node(position, Box::new(node))
    }

    pub fn get_node(&self, id: &Id<ShaderGraphNodeData>) -> Option<&ShaderGraphNodeData> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(
        &mut self,
        id: &Id<ShaderGraphNodeData>,
    ) -> Option<&mut ShaderGraphNodeData> {
        self.nodes.get_mut(id)
    }

    pub fn connect_slots(
        &mut self,
        from: Id<ShaderGraphOutputSlotData>,
        to: Id<ShaderGraphInputSlotData>,
    ) {
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
        from: Id<ShaderGraphOutputSlotData>,
        to: Id<ShaderGraphInputSlotData>,
    ) -> bool {
        let from_slot = self.slots.outputs.get(&from);
        let to_slot = self.slots.inputs.get(&to);

        if let (Some(from), Some(to)) = (from_slot, to_slot) {
            from.data.ty().name() == to.data.ty().name()
                || self.can_cast(from.data.ty(), to.data.ty())
        } else {
            false
        }
    }

    pub fn can_cast(
        &self,
        from: &dyn ErasedShaderGraphValueType,
        to: &dyn ErasedShaderGraphValueType,
    ) -> bool {
        let from_name = from.name();
        let to_name = to.name();
        self.casters
            .casters
            .get(from_name)
            .and_then(|map| map.get(to_name))
            .is_some()
    }

    pub fn disconnect_slot(&mut self, to: Id<ShaderGraphInputSlotData>) {
        if let Some(input_slot) = self.slots.inputs.get_mut(&to) {
            input_slot.connected = None;
            self.invalidate_cache();
        }
    }

    pub fn connect_slots_by_index(
        &mut self,
        from_node: Id<ShaderGraphNodeData>,
        from_output_index: usize,
        to_node: Id<ShaderGraphNodeData>,
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

    pub fn disconnect_slots_by_index(
        &mut self,
        to_node: Id<ShaderGraphNodeData>,
        to_input_index: usize,
    ) {
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

    pub fn compile(&mut self) -> Result<String, ShaderGraphCompileError> {
        if self.cached_run_order.is_none() {
            self.update_cache();
        }

        let run_order = self.cached_run_order.as_ref().unwrap();
        let mut code = String::new();
        for node_id in run_order.clone() {
            let node = self.nodes.get(&node_id).unwrap();
            let context = ShaderGraphNodeCodeGenContext {
                inputs: &node.inputs,
                outputs: &node.outputs,
                graph_slots: &mut self.slots,
                casters: &self.casters,
            };

            match node.data.generate_code(context) {
                Ok(node_code) => code.push_str(&node_code),
                Err(err) => {
                    return Err(ShaderGraphCompileError::NodeCodeGenError(
                        ContextualShaderGraphNodeCodeGenError {
                            node_id,
                            node_title: node.data.title().to_string(),
                            err,
                            code: code.clone(),
                        },
                    ));
                }
            }
        }
        self.signature
            .compile(code)
            .ok_or(ShaderGraphCompileError::InvalidFunctionSignature)
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

    pub fn update_literal(&mut self, message: ErasedShaderGraphLiteralUpdateMessage) {
        if let Some(slot) = self.slots.inputs.get_mut(&message.id) {
            slot.data.ty.update_literal(&mut slot.data.value, message);
        }
    }

    pub fn add_caster<T: ShaderVariableCaster + Default>(&mut self) {
        self.casters.register::<T>();
    }
}

#[derive(Default)]
pub struct ShaderGraphSlots {
    inputs: HashMap<Id<ShaderGraphInputSlotData>, ShaderGraphInputSlotData>,
    outputs: HashMap<Id<ShaderGraphOutputSlotData>, ShaderGraphOutputSlotData>,
}

impl ShaderGraphSlots {
    pub fn get_input(
        &self,
        id: &Id<ShaderGraphInputSlotData>,
    ) -> Option<&ShaderGraphInputSlotData> {
        self.inputs.get(id)
    }

    pub fn get_output(
        &self,
        id: &Id<ShaderGraphOutputSlotData>,
    ) -> Option<&ShaderGraphOutputSlotData> {
        self.outputs.get(id)
    }

    pub fn get_connected(
        &self,
        input_id: &Id<ShaderGraphInputSlotData>,
    ) -> Option<&ShaderGraphOutputSlotData> {
        let input_node = self.inputs.get(input_id)?;
        let connected_id = input_node.connected.as_ref()?;
        self.outputs.get(connected_id)
    }
}

pub struct ShaderGraphFunctionSignature {
    name: String,
    ret_type: Box<dyn ErasedShaderGraphValueType>,
    params: Vec<ShaderVariable>,
}

impl ShaderGraphFunctionSignature {
    pub fn new<T: ShaderGraphValueType>(name: String, ret_type: T) -> Self {
        Self {
            name,
            ret_type: Box::new(ret_type),
            params: Vec::new(),
        }
    }

    pub fn with_param<T: ShaderGraphValueType + Default>(mut self, identifier: String) -> Self {
        self.params.push(ShaderVariable::new::<T>(identifier));
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

pub trait ShaderGraphNodeCreator {
    type NodeType: ShaderGraphNode;
    fn create(&self) -> Self::NodeType;
}

pub trait ErasedShaderGraphNodeCreator {
    fn create(&self) -> Box<dyn ShaderGraphNode>;
}

impl<T: ShaderGraphNodeCreator> ErasedShaderGraphNodeCreator for T {
    fn create(&self) -> Box<dyn ShaderGraphNode> {
        Box::new(self.create())
    }
}

pub struct ShaderGraphNodeData {
    pub position: Point,
    pub data: Box<dyn ShaderGraphNode>,
    pub inputs: Vec<Id<ShaderGraphInputSlotData>>,
    pub outputs: Vec<Id<ShaderGraphOutputSlotData>>,
}

pub trait ShaderGraphNode: Send + Sync + 'static {
    fn title(&self) -> &str;
    fn title_color(&self) -> Color;
    fn create_inputs(&self) -> Vec<ShaderGraphDefaultInputSlot>;
    fn create_outputs(&self) -> Vec<ShaderGraphDefaultOutputSlot>;
    fn generate_code(
        &self,
        ctx: ShaderGraphNodeCodeGenContext,
    ) -> Result<String, ShaderGraphNodeCodeGenError>;
}

pub struct ShaderGraphNodeCodeGenContext<'a> {
    pub inputs: &'a [Id<ShaderGraphInputSlotData>],
    pub outputs: &'a [Id<ShaderGraphOutputSlotData>],
    pub graph_slots: &'a mut ShaderGraphSlots,
    pub casters: &'a ShaderGraphCasters,
}

impl ShaderGraphNodeCodeGenContext<'_> {
    pub fn get_input<const N: usize>(&self) -> Result<String, ShaderGraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(N)
            .ok_or(ShaderGraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(ShaderGraphNodeCodeGenError::MissingInputSlot)?;

        let Some(connected) = slot.connected else {
            // Literal value should always has the same type as the slot type.
            return slot
                .data
                .to_code()
                .ok_or(ShaderGraphNodeCodeGenError::LiteralToCodeFailed);
        };

        let output_slot = self
            .graph_slots
            .get_output(&connected)
            .ok_or(ShaderGraphNodeCodeGenError::MissingOutputSlot)?;

        if output_slot.data.ty().name() != slot.data.ty().name() {
            self.casters
                .try_cast(&output_slot.data, slot.data.ty())
                .ok_or(ShaderGraphNodeCodeGenError::FailedToCastVariable)
        } else {
            Ok(output_slot.data.identifier.clone())
        }
    }

    pub fn get_input_raw<const N: usize, T: 'static>(
        &self,
    ) -> Result<&T, ShaderGraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(N)
            .ok_or(ShaderGraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(ShaderGraphNodeCodeGenError::MissingInputSlot)?;

        Ok(slot.data.as_ref::<T>())
    }

    pub fn get_output<const N: usize>(&self) -> Result<String, ShaderGraphNodeCodeGenError> {
        let slot_id = self
            .outputs
            .get(N)
            .ok_or(ShaderGraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_output(slot_id)
            .ok_or(ShaderGraphNodeCodeGenError::MissingOutputSlot)?;

        Ok(slot.data.identifier.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderGraphSlotType {
    Normal,
    Unconnectable,
    Hidden,
}

pub struct ShaderGraphDefaultInputSlot {
    pub name: &'static str,
    pub value: ShaderLiteral,
    pub slot_type: ShaderGraphSlotType,
}

impl ShaderGraphDefaultInputSlot {
    pub fn new<T: ShaderGraphValueType + Default>(
        name: &'static str,
        value: T::AssociatedLiteralType,
    ) -> Self {
        Self {
            name,
            value: ShaderLiteral::new::<T>(value),
            slot_type: ShaderGraphSlotType::Normal,
        }
    }

    pub fn unconnectable<T: ShaderGraphValueType + Default>(
        name: &'static str,
        value: T::AssociatedLiteralType,
    ) -> Self {
        Self {
            name,
            value: ShaderLiteral::new::<T>(value),
            slot_type: ShaderGraphSlotType::Unconnectable,
        }
    }

    pub fn hidden<T: ShaderGraphValueType + Default>(value: T::AssociatedLiteralType) -> Self {
        Self {
            name: Default::default(),
            value: ShaderLiteral::new::<T>(value),
            slot_type: ShaderGraphSlotType::Hidden,
        }
    }

    pub fn new_non_default<T: ShaderGraphValueType>(
        name: &'static str,
        value: T::AssociatedLiteralType,
        ty: T,
        slot_type: ShaderGraphSlotType,
    ) -> Self {
        Self {
            name,
            value: ShaderLiteral::new_non_default::<T>(value, ty),
            slot_type,
        }
    }
}

pub struct ShaderGraphInputSlotData {
    pub node_id: Id<ShaderGraphNodeData>,
    pub name: &'static str,
    pub data: ShaderLiteral,
    pub connected: Option<Id<ShaderGraphOutputSlotData>>,
    pub slot_type: ShaderGraphSlotType,
}

pub struct ShaderGraphDefaultOutputSlot {
    pub name: &'static str,
    pub ty: Box<dyn ErasedShaderGraphValueType>,
}

impl ShaderGraphDefaultOutputSlot {
    pub fn new<T: ShaderGraphValueType + Default>(name: &'static str) -> Self {
        Self {
            name,
            ty: Box::new(T::default()),
        }
    }
}

pub struct ShaderGraphOutputSlotData {
    pub node_id: Id<ShaderGraphNodeData>,
    pub name: &'static str,
    pub data: ShaderVariable,
}

pub trait ShaderGraphValueType: Send + Sync + 'static {
    type AssociatedLiteralType: Send + Sync + 'static;
    type Message: Send + Sync + 'static;
    fn color(&self) -> Color;
    fn name(&self) -> &'static str;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, ShaderGraphTheme, ShaderGraphRenderer>;
    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message);
    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String>;
}

#[derive(Debug)]
pub struct ErasedShaderGraphLiteralUpdateMessage {
    pub inner: Box<dyn Any + Send + Sync>,
    pub id: Id<ShaderGraphInputSlotData>,
}

pub trait ErasedShaderGraphValueType: Send + Sync + 'static {
    fn color(&self) -> Color;
    fn name(&self) -> &'static str;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn view_literal(
        &self,
        slot_id: Id<ShaderGraphInputSlotData>,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Element<
        'static,
        ErasedShaderGraphLiteralUpdateMessage,
        ShaderGraphTheme,
        ShaderGraphRenderer,
    >;
    fn update_literal(
        &self,
        data: &mut Box<dyn Any + Send + Sync>,
        message: ErasedShaderGraphLiteralUpdateMessage,
    );
    fn literal_to_code(&self, data: &Box<dyn Any + Send + Sync>) -> Option<String>;
}

impl<T: ShaderGraphValueType> ErasedShaderGraphValueType for T {
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
        slot_id: Id<ShaderGraphInputSlotData>,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Element<
        'static,
        ErasedShaderGraphLiteralUpdateMessage,
        ShaderGraphTheme,
        ShaderGraphRenderer,
    > {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.view_literal(literal)
            .map(move |msg| ErasedShaderGraphLiteralUpdateMessage {
                inner: Box::new(msg),
                id: slot_id,
            })
    }

    fn update_literal(
        &self,
        data: &mut Box<dyn Any + Send + Sync>,
        message: ErasedShaderGraphLiteralUpdateMessage,
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

pub struct ShaderLiteral {
    value: Box<dyn Any + Send + Sync>,
    ty: Box<dyn ErasedShaderGraphValueType>,
}

impl ShaderLiteral {
    pub fn new<T: ShaderGraphValueType + Default>(value: T::AssociatedLiteralType) -> Self {
        Self {
            value: Box::new(value),
            ty: Box::new(T::default()),
        }
    }

    pub fn new_non_default<T: ShaderGraphValueType>(
        value: T::AssociatedLiteralType,
        ty: T,
    ) -> Self {
        Self {
            value: Box::new(value),
            ty: Box::new(ty),
        }
    }

    pub fn as_ref<T: 'static>(&self) -> &T {
        self.value
            .downcast_ref::<T>()
            .expect("Failed to downcast ShaderLiteral")
    }

    pub fn as_mut<T: 'static>(&mut self) -> &mut T {
        self.value
            .downcast_mut::<T>()
            .expect("Failed to downcast ShaderLiteral")
    }

    pub fn ty(&self) -> &dyn ErasedShaderGraphValueType {
        self.ty.as_ref()
    }

    pub fn set<T: 'static>(&mut self, value: T) {
        if let Some(x) = self.value.downcast_mut() {
            *x = value;
        } else {
            log::error!("Setting a ShaderLiteral with a different type");
        }
    }

    pub fn to_code(&self) -> Option<String> {
        self.ty.literal_to_code(&self.value)
    }
}

pub struct ShaderVariable {
    identifier: String,
    ty: Box<dyn ErasedShaderGraphValueType>,
}

impl ShaderVariable {
    pub fn new<T: ShaderGraphValueType + Default>(identifier: String) -> Self {
        Self {
            identifier,
            ty: Box::new(T::default()),
        }
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn ty(&self) -> &dyn ErasedShaderGraphValueType {
        self.ty.as_ref()
    }
}

#[derive(Default)]
pub struct ShaderGraphCasters {
    casters: HashMap<&'static str, HashMap<&'static str, Box<dyn ErasedShaderVariableCaster>>>,
}

impl ShaderGraphCasters {
    pub fn register<T: ShaderVariableCaster + Default>(&mut self) {
        let from = T::FromType::default();
        let to = T::ToType::default();
        let from_name = <T::FromType as ShaderGraphValueType>::name(&from);
        let to_name = <T::ToType as ShaderGraphValueType>::name(&to);
        let caster: Box<dyn ErasedShaderVariableCaster> = Box::new(T::default());
        self.casters
            .entry(from_name)
            .or_default()
            .insert(to_name, caster);
    }

    pub fn try_cast(
        &self,
        variable: &ShaderVariable,
        to_type: &dyn ErasedShaderGraphValueType,
    ) -> Option<String> {
        let from_name = variable.ty().name();
        let to_name = to_type.name();
        let caster = self.casters.get(from_name)?.get(to_name)?;
        Some(caster.cast(&variable.identifier))
    }
}

pub trait ShaderVariableCaster: 'static {
    type FromType: ShaderGraphValueType + Default;
    type ToType: ShaderGraphValueType + Default;
    fn cast(&self, variable: &String) -> String;
}

pub trait ErasedShaderVariableCaster {
    fn cast(&self, variable: &String) -> String;
}

impl<T: ShaderVariableCaster> ErasedShaderVariableCaster for T {
    fn cast(&self, variable: &String) -> String {
        self.cast(variable)
    }
}

#[derive(Default)]
pub struct ShaderGraphIdentifierGenerator {
    counter: usize,
}

impl ShaderGraphIdentifierGenerator {
    pub fn next_output(&mut self) -> String {
        let ident = format!("output_{}", self.counter);
        self.counter += 1;
        ident
    }
}
