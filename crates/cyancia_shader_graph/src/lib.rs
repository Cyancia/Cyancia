use std::{
    any::Any,
    collections::{HashMap, VecDeque, hash_map::Entry},
};

use cyancia_id::Id;
use cyancia_utils::wrapper;
use iced_core::{Color, Element, Point};

pub mod editor;

pub type ShaderGraphTheme = iced_core::Theme;
pub type ShaderGraphRenderer = iced_wgpu::Renderer;

#[derive(Debug, thiserror::Error)]
pub enum ShaderGraphError {
    #[error("Node not found: {0:?}")]
    NodeNotFound(Id<ShaderGraphNodeData>),
}

#[derive(Default)]
pub struct ShaderGraph {
    nodes: HashMap<Id<ShaderGraphNodeData>, ShaderGraphNodeData>,
    slots: ShaderGraphSlots,
    casters: ShaderGraphCasters,
    ident_generator: ShaderGraphIdentifierGenerator,
    cached_run_order: Option<Vec<Id<ShaderGraphNodeData>>>,
}

impl ShaderGraph {
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

    pub fn connect_slot(
        &mut self,
        from: Id<ShaderGraphOutputSlotData>,
        to: Id<ShaderGraphInputSlotData>,
    ) {
        if let Some(input_slot) = self.slots.inputs.get_mut(&to) {
            input_slot.connected = Some(from);
            self.invalidate_cache();
        }
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
            self.connect_slot(from, to);
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

    pub fn compile(&mut self) -> Result<String, ShaderGraphError> {
        if self.cached_run_order.is_none() {
            self.update_cache();
        }

        let run_order = self.cached_run_order.as_ref().unwrap();
        let mut code = String::new();
        for node_id in run_order.clone() {
            let node_code = self.run_node(node_id)?;
            code.push_str(&node_code);
            code.push('\n');
        }
        Ok(code)
    }

    pub fn run_node(&mut self, id: Id<ShaderGraphNodeData>) -> Result<String, ShaderGraphError> {
        let node = self
            .nodes
            .get(&id)
            .ok_or(ShaderGraphError::NodeNotFound(id))?;
        let context = ShaderGraphNodeCodeGenContext {
            inputs: &node.inputs,
            outputs: &node.outputs,
            graph_slots: &mut self.slots,
            casters: &self.casters,
        };
        Ok(node.data.generate_code(context))
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
        let mut ready_nodes = out_degrees
            .iter()
            .filter(|(_, deg)| **deg == 0)
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

pub trait ShaderGraphNodeCreator {
    type NodeType: ShaderGraphNode + Default;
    fn create(&self) -> Self::NodeType {
        Self::NodeType::default()
    }
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
    fn generate_code(&self, ctx: ShaderGraphNodeCodeGenContext) -> String;
}

pub struct ShaderGraphNodeCodeGenContext<'a> {
    pub inputs: &'a [Id<ShaderGraphInputSlotData>],
    pub outputs: &'a [Id<ShaderGraphOutputSlotData>],
    pub graph_slots: &'a mut ShaderGraphSlots,
    pub casters: &'a ShaderGraphCasters,
}

impl ShaderGraphNodeCodeGenContext<'_> {
    pub fn get_input<const N: usize>(&self) -> Option<String> {
        let slot_id = self.inputs.get(N)?;
        let slot = self.graph_slots.get_input(slot_id)?;
        let Some(connected) = slot.connected else {
            // Literal value should always has the same type as the slot type.
            return Some(slot.data.to_string());
        };

        let output_slot = self.graph_slots.get_output(&connected)?;
        if output_slot.data.ty().name() != slot.data.ty().name() {
            self.casters.try_cast(&output_slot.data, slot.data.ty())
        } else {
            Some(output_slot.data.identifier.clone())
        }
    }

    pub fn get_output<const N: usize>(&self) -> Option<String> {
        let slot_id = self.outputs.get(N)?;
        let slot = self.graph_slots.get_output(slot_id)?;
        Some(slot.data.identifier.clone())
    }
}

pub struct ShaderGraphDefaultInputSlot {
    pub name: &'static str,
    pub value: ShaderLiteral,
}

impl ShaderGraphDefaultInputSlot {
    pub fn new<T: ShaderGraphValueType + Default>(
        name: &'static str,
        value: T::AssociatedLiteralType,
    ) -> Self {
        Self {
            name,
            value: ShaderLiteral::new::<T>(value),
        }
    }
}

pub struct ShaderGraphInputSlotData {
    pub node_id: Id<ShaderGraphNodeData>,
    pub name: &'static str,
    pub data: ShaderLiteral,
    pub connected: Option<Id<ShaderGraphOutputSlotData>>,
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
    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, ShaderGraphTheme, ShaderGraphRenderer>;
    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message);
    fn literal_to_string(&self, data: &Self::AssociatedLiteralType) -> String;
}

#[derive(Debug)]
pub struct ErasedShaderGraphLiteralUpdateMessage {
    pub inner: Box<dyn Any + Send + Sync>,
    pub id: Id<ShaderGraphInputSlotData>,
}

pub trait ErasedShaderGraphValueType: Send + Sync + 'static {
    fn color(&self) -> Color;
    fn name(&self) -> &'static str;
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
    fn literal_to_string(&self, data: &Box<dyn Any + Send + Sync>) -> String;
}

impl<T: ShaderGraphValueType> ErasedShaderGraphValueType for T {
    fn color(&self) -> Color {
        self.color()
    }

    fn name(&self) -> &'static str {
        self.name()
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

    fn literal_to_string(&self, data: &Box<dyn Any + Send + Sync>) -> String {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.literal_to_string(literal)
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

    pub fn to_string(&self) -> String {
        self.ty.literal_to_string(&self.value)
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
        Some(caster.cast(variable))
    }
}

pub trait ShaderVariableCaster: 'static {
    type FromType: ShaderGraphValueType + Default;
    type ToType: ShaderGraphValueType + Default;
    fn cast(&self, variable: &ShaderVariable) -> String;
}

pub trait ErasedShaderVariableCaster {
    fn cast(&self, variable: &ShaderVariable) -> String;
}

impl<T: ShaderVariableCaster> ErasedShaderVariableCaster for T {
    fn cast(&self, variable: &ShaderVariable) -> String {
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
