use std::{
    any::Any,
    collections::{BTreeMap, HashMap, hash_map::Entry},
    rc::Rc,
    sync::Arc,
};

use cyancia_utils::{cloneable_any::ClonableAnySync, wrapper};
use dyn_clone::DynClone;
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, Pixels, Point, Rgba,
    Styled, WeakEntity, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::ElementExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    editor::GraphEditor,
    graph::{
        Graph, GraphData, GraphResources, GraphSignature, GraphVarIdentGenerator,
        slot::{
            GraphDefaultInputSlot, GraphDefaultOutputSlot, GraphInlineLiteralRenderContext,
            GraphInputSlotId, GraphOutputSlotId, GraphSlots, GraphValueType,
        },
        texture::GraphTextureUsageRecorder,
        variable::{GraphLiteral, GraphLiteralValue, GraphTypeRegistry, GraphVariable},
    },
    save::GraphSerializable,
};

pub use cyancia_shader_graph_derive::stateless;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub GraphNodeId : Uuid
}

pub trait GraphNode<Data: GraphData>: Send + Sync + 'static + DynClone {
    type State: Send + Sync + 'static + GraphSerializable;
    fn name(&self) -> &'static str;
    fn default_state(&self) -> Self::State;
    fn header_color(&self, cx: &App) -> Rgba;
    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot>;
    fn update_signature(&self, _: &Self::State, _: GraphNodeUpdateSignatureContext<'_, Data>) {}
    fn render(&self, state: &Self::State, ctx: GraphNodeRenderContext<'_, '_, Data>) -> AnyElement;
    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError>;
    fn run(
        &self,
        _: &Self::State,
        _: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        Err(GraphNodeRunError::Unavailable)
    }
    fn serialize_state(&self, state: &Self::State) -> Result<toml::Value, toml::ser::Error> {
        state.to_toml()
    }
    fn deserialize_state(
        &self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Self::State, toml::de::Error> {
        Self::State::from_toml(value, type_registry)
    }
}

#[derive(Clone)]
pub struct ErasedGraphNodeMessage {
    pub inner: Box<dyn ClonableAnySync>,
    pub id: GraphNodeId,
}

pub trait ErasedGraphNode<Data: GraphData>: Send + Sync + 'static + DynClone {
    fn name(&self) -> &'static str;
    fn default_state(&self) -> Box<dyn Any + Send + Sync>;
    fn header_color(&self, cx: &App) -> Rgba;
    fn create_inputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot>;
    fn update_signature(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeUpdateSignatureContext<'_, Data>,
    );
    fn render(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement;
    fn generate_code(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError>;
    fn run(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError>;
    fn serialize_state(
        &self,
        state: &Box<dyn Any + Send + Sync>,
    ) -> Result<toml::Value, toml::ser::Error>;
    fn deserialize_state(
        &self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Box<dyn Any + Send + Sync>, toml::de::Error>;
}

dyn_clone::clone_trait_object!(<Data> ErasedGraphNode<Data>);

impl<T: GraphNode<Data>, Data: GraphData> ErasedGraphNode<Data> for T {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn default_state(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(self.default_state())
    }

    fn header_color(&self, cx: &App) -> Rgba {
        self.header_color(cx)
    }

    fn create_inputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.create_inputs(state, ctx)
    }

    fn create_outputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.create_outputs(state, ctx)
    }

    fn update_signature(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeUpdateSignatureContext<'_, Data>,
    ) {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.update_signature(state, ctx);
    }

    fn render(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.render(state, ctx)
    }

    fn generate_code(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.generate_code(state, ctx)
    }

    fn run(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.run(state, ctx)
    }

    fn serialize_state(
        &self,
        state: &Box<dyn Any + Send + Sync>,
    ) -> Result<toml::Value, toml::ser::Error> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.serialize_state(state)
    }

    fn deserialize_state(
        &self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Box<dyn Any + Send + Sync>, toml::de::Error> {
        let state = self.deserialize_state(value, type_registry)?;
        Ok(Box::new(state))
    }
}

pub struct StatefulGraphNode<Data: GraphData> {
    state: Box<dyn Any + Send + Sync>,
    data: Box<dyn ErasedGraphNode<Data>>,
}

impl<Data: GraphData> StatefulGraphNode<Data> {
    pub fn new(node: Box<dyn ErasedGraphNode<Data>>) -> Self {
        Self {
            state: node.default_state(),
            data: node,
        }
    }

    pub fn state<T: GraphNode<Data>>(&self) -> Option<&T::State> {
        self.state.downcast_ref()
    }

    pub fn state_mut<T: GraphNode<Data>>(&mut self) -> Option<&mut T::State> {
        self.state.downcast_mut()
    }

    pub fn name(&self) -> &'static str {
        self.data.name()
    }

    pub fn header_color(&self, cx: &App) -> Rgba {
        self.data.header_color(cx)
    }

    pub fn render(&self, ctx: GraphNodeRenderContext<'_, '_, Data>) -> AnyElement {
        self.data.render(&self.state, ctx)
    }

    pub fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        self.data.generate_code(&self.state, ctx)
    }

    pub fn run(&self, ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        self.data.run(&self.state, ctx)
    }

    pub fn serialize_state(&self) -> Result<toml::Value, toml::ser::Error> {
        self.data.serialize_state(&self.state)
    }

    pub fn deserialize_and_set_state(
        &mut self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<(), toml::de::Error> {
        let state = self.data.deserialize_state(value, type_registry)?;
        self.state = state;
        Ok(())
    }

    pub fn create_inputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        self.data.create_inputs(&self.state, ctx)
    }

    pub fn create_outputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        self.data.create_outputs(&self.state, ctx)
    }

    pub fn update_signature(&self, ctx: GraphNodeUpdateSignatureContext<'_, Data>) {
        self.data.update_signature(&self.state, ctx);
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct StatelessState {
    #[serde(skip)]
    _private: (),
}

pub trait StatelessCommonGraphNode<Data: GraphData>: Send + Sync + 'static + DynClone {
    fn name(&self) -> &'static str;
    fn header_color(&self, cx: &App) -> Rgba;
    fn create_inputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot>;
    fn update_signature(&self, _: GraphNodeUpdateSignatureContext<'_, Data>) {}
    fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError>;
    fn run(&self, _: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Err(GraphNodeRunError::Unavailable)
    }
}

pub struct GraphNodeData<Data: GraphData> {
    pub position: Point<f32>,
    pub data: StatefulGraphNode<Data>,
    pub inputs: Arc<[GraphInputSlotId]>,
    pub outputs: Arc<[GraphOutputSlotId]>,
}

impl<Data: GraphData> GraphNodeData<Data> {
    pub fn render(
        &self,
        node_id: GraphNodeId,
        graph_slots: &GraphSlots,
        resources: &GraphResources<Data>,
        type_registry: &GraphTypeRegistry,
        editor: WeakEntity<GraphEditor<Data>>,
        window: &mut Window,
        cx: &mut Context<'_, Graph<Data>>,
    ) -> AnyElement {
        self.data.render(GraphNodeRenderContext {
            node_id,
            inputs: &self.inputs,
            outputs: &self.outputs,
            graph_slots,
            resources,
            type_registry,
            editor,
            window,
            cx,
        })
    }
}

pub struct GraphNodeCreateSlotsContext<'a, Data: GraphData> {
    pub resources: &'a GraphResources<Data>,
    pub type_registry: &'a GraphTypeRegistry,
    pub cx: &'a App,
}

pub struct GraphNodeUpdateSignatureContext<'a, Data: GraphData> {
    pub inputs: &'a [GraphInputSlotId],
    pub outputs: &'a [GraphOutputSlotId],
    pub slots: &'a GraphSlots,
    pub signature: &'a mut GraphSignature,
    pub type_registry: &'a GraphTypeRegistry,
    pub resources: &'a GraphResources<Data>,
}

impl<Data: GraphData> GraphNodeUpdateSignatureContext<'_, Data> {
    pub fn require_output_slot_as_graph_input(&mut self, index: usize, name: String) {
        let Some(slot_id) = self.outputs.get(index) else {
            return;
        };
        let Some(slot) = self.slots.outputs.get(slot_id) else {
            return;
        };

        self.signature.inputs.insert(
            *slot_id,
            GraphVariable::new_boxed(name, dyn_clone::clone_box(&*slot.data_ty)),
        );
    }

    pub fn require_input_slot_as_graph_output(&mut self, index: usize, name: String) {
        let Some(slot_id) = self.inputs.get(index) else {
            return;
        };
        let Some(slot) = self.slots.inputs.get(slot_id) else {
            return;
        };

        self.signature.outputs.insert(
            *slot_id,
            GraphVariable::new_boxed(name, dyn_clone::clone_box(slot.data.ty())),
        );
    }
}

pub struct GraphNodeRenderContext<'a, 'app, Data: GraphData> {
    pub node_id: GraphNodeId,
    pub inputs: &'a [GraphInputSlotId],
    pub outputs: &'a [GraphOutputSlotId],
    pub graph_slots: &'a GraphSlots,
    pub resources: &'a GraphResources<Data>,
    pub type_registry: &'a GraphTypeRegistry,
    pub editor: WeakEntity<GraphEditor<Data>>,
    pub window: &'a mut Window,
    pub cx: &'a mut Context<'app, Graph<Data>>,
}

const NODE_HEADER_GAP: Pixels = px(2.0);
const SLOT_SECTION_GAP: Pixels = px(2.0);
const SLOT_ROW_GAP: Pixels = px(3.0);
const SLOT_STACK_GAP: Pixels = px(1.0);
const SLOT_DOT_SIZE: Pixels = px(10.0);
const SLOT_DOT_RADIUS: Pixels = px(5.0);

impl<Data: GraphData> GraphNodeRenderContext<'_, '_, Data> {
    pub fn render_all_slots_with_header(&mut self, header: impl IntoElement) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(NODE_HEADER_GAP)
            .child(
                div()
                    .w_full()
                    .child(header)
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation()),
            )
            .child(self.render_all_slots())
            .into_any_element()
    }

    pub fn render_all_slots(&mut self) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(SLOT_SECTION_GAP)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SLOT_STACK_GAP)
                    .children((0..self.inputs.len()).map(|i| self.render_input_slot(i).unwrap())),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SLOT_STACK_GAP)
                    .children((0..self.outputs.len()).map(|i| self.render_output_slot(i).unwrap())),
            )
            .into_any_element()
    }

    pub fn render_input_slot(&mut self, index: usize) -> Option<AnyElement> {
        let slot_id = *self.inputs.get(index)?;
        let slot = self.graph_slots.get_input(&slot_id)?;

        Some(
            div()
                .id(*slot_id)
                .flex()
                .items_center()
                .gap(SLOT_ROW_GAP)
                .child(
                    div()
                        .bg(slot.data.ty().color(self.cx))
                        .min_w(SLOT_DOT_SIZE)
                        .min_h(SLOT_DOT_SIZE)
                        .rounded(SLOT_DOT_RADIUS)
                        .on_prepaint({
                            let editor = self.editor.clone();
                            move |bounds, _, cx| {
                                let _ = editor.update(cx, |editor, _| {
                                    editor.add_input_slot_pos(slot_id, bounds.center());
                                });
                            }
                        }),
                )
                .child(div().text_sm().child(slot.name.clone()))
                .when(slot.connected.is_none(), |d| {
                    d.child(slot.data.ty().render_inline(
                        slot.data.value(),
                        GraphInlineLiteralRenderContext {
                            slot_id,
                            window: self.window,
                            on_update: Rc::new({
                                let graph = self.cx.entity().downgrade();
                                move |value, cx| {
                                    let _ = graph.update(cx, |graph, _| {
                                        graph.set_slot_value(slot_id, value);
                                    });
                                }
                            }),
                            cx: self.cx,
                        },
                    ))
                })
                .into_any_element(),
        )
    }

    pub fn render_output_slot(&mut self, index: usize) -> Option<AnyElement> {
        let slot_id = *self.outputs.get(index)?;
        let slot = self.graph_slots.get_output(&slot_id)?;

        Some(
            div()
                .id(*slot_id)
                .flex()
                .items_center()
                .justify_end()
                .gap(SLOT_ROW_GAP)
                .child(div().text_sm().child(slot.name.clone()))
                .child(
                    div()
                        .bg(slot.data_ty.color(self.cx))
                        .min_w(SLOT_DOT_SIZE)
                        .min_h(SLOT_DOT_SIZE)
                        .rounded(SLOT_DOT_RADIUS)
                        .on_prepaint({
                            let editor = self.editor.clone();
                            move |bounds, _, cx| {
                                let _ = editor.update(cx, |editor, _| {
                                    editor.add_output_slot_pos(slot_id, bounds.center());
                                });
                            }
                        }),
                )
                .into_any_element(),
        )
    }
}

pub struct GraphNodeRunContext<'a, Data: GraphData> {
    pub data: &'a Data,
    pub inputs: &'a [GraphInputSlotId],
    pub outputs: &'a [GraphOutputSlotId],
    pub graph_slots: &'a GraphSlots,
    pub output_storage: &'a mut HashMap<GraphOutputSlotId, GraphLiteral>,
    pub resources: &'a GraphResources<Data>,
    pub type_registry: &'a GraphTypeRegistry,
    pub cx: &'a App,
}

impl<'a, Data: GraphData> GraphNodeRunContext<'a, Data> {
    pub fn get_input_value<T: GraphValueType>(
        &self,
        index: usize,
    ) -> Result<T::AssociatedLiteralType, GraphNodeRunError> {
        let slot_id = self
            .inputs
            .get(index)
            .ok_or(GraphNodeRunError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeRunError::MissingInputSlot)?;

        if let Some(connected) = slot.connected {
            let connected_value = self
                .output_storage
                .get(&connected)
                .ok_or(GraphNodeRunError::MissingOutputSlot)?;

            if connected_value.ty().name() != slot.data.ty().name() {
                let casted = self
                    .type_registry
                    .try_cast(
                        connected_value.ty(),
                        slot.data.ty(),
                        connected_value.value(),
                    )
                    .ok_or(GraphNodeRunError::FailedToCastVariable)?;
                casted
                    .downcast::<T::AssociatedLiteralType>()
                    .map(|v| *v)
                    .map_err(|_| GraphNodeRunError::FailedToCastVariable)
            } else {
                Ok(connected_value
                    .clone()
                    .downcast::<T::AssociatedLiteralType>())
            }
        } else {
            Ok(slot.data.clone().downcast::<T::AssociatedLiteralType>())
        }
    }

    pub fn get_input_value_raw(&self, index: usize) -> Result<&GraphLiteral, GraphNodeRunError> {
        let slot_id = self
            .inputs
            .get(index)
            .ok_or(GraphNodeRunError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeRunError::MissingInputSlot)?;

        if let Some(connected) = slot.connected {
            self.output_storage
                .get(&connected)
                .ok_or(GraphNodeRunError::MissingOutputSlot)
        } else {
            Ok(&slot.data)
        }
    }

    pub fn set_output_value<T: GraphValueType + Default>(
        &mut self,
        index: usize,
        value: T::AssociatedLiteralType,
    ) -> Result<(), GraphNodeRunError> {
        let slot_id = self
            .outputs
            .get(index)
            .ok_or(GraphNodeRunError::SlotIndexOutOfBounds)?;

        self.output_storage
            .insert(*slot_id, GraphLiteral::new::<T>(value));
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GraphNodeRunError {
    #[error("Node cannot run on CPU")]
    Unavailable,
    #[error("Input slot index out of bounds")]
    SlotIndexOutOfBounds,
    #[error("Missing input slot")]
    MissingInputSlot,
    #[error("Missing output slot")]
    MissingOutputSlot,
    #[error("Failed to cast variable")]
    FailedToCastVariable,
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
}

pub struct GraphNodeCodeGenContext<'a, Data: GraphData> {
    pub inputs: &'a [GraphInputSlotId],
    pub outputs: &'a [GraphOutputSlotId],
    pub graph_slots: &'a GraphSlots,
    pub output_slot_idents: &'a mut HashMap<GraphOutputSlotId, String>,
    pub ident_generator: &'a mut GraphVarIdentGenerator,
    pub resources: &'a GraphResources<Data>,
    pub type_registry: &'a GraphTypeRegistry,
    pub texture_usage: &'a mut GraphTextureUsageRecorder,
    pub cx: &'a App,
}

impl<Data: GraphData> GraphNodeCodeGenContext<'_, Data> {
    pub fn get_input(&self, index: usize) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(index)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        let Some(connected) = slot.connected else {
            dbg!();
            // Literal value should always has the same type as the slot type.
            return slot
                .data
                .to_code()
                .ok_or(GraphNodeCodeGenError::LiteralToCodeFailed);
        };
        dbg!();

        let output_slot = self
            .graph_slots
            .get_output(&connected)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        let ident = self
            .output_slot_idents
            .get(&connected)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        if output_slot.data_ty.name() != slot.data.ty().name() {
            self.type_registry
                .try_wgsl_cast(&*output_slot.data_ty, slot.data.ty(), ident)
                .ok_or(GraphNodeCodeGenError::FailedToCastVariable)
        } else {
            Ok(ident.clone())
        }
    }

    pub fn get_input_raw<T: GraphLiteralValue>(
        &self,
        index: usize,
    ) -> Result<&T, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(index)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        Ok(slot.data.as_ref::<T>())
    }

    pub fn get_output(&mut self, index: usize) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .outputs
            .get(index)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let ident = match self.output_slot_idents.entry(*slot_id) {
            Entry::Occupied(e) => e.get().clone(),
            Entry::Vacant(e) => e.insert(self.ident_generator.next_output()).clone(),
        };
        Ok(ident)
    }
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
    pub node_id: GraphNodeId,
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

#[derive(Debug)]
pub struct ContextualGraphNodeRunError {
    pub node_id: GraphNodeId,
    pub node_title: String,
    pub err: GraphNodeRunError,
}

impl std::fmt::Display for ContextualGraphNodeRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error in node {:?} of type {}: {:?}",
            self.node_id, self.node_title, self.err
        )
    }
}

#[derive(Default)]
pub struct GraphNodeRegistry<Data: GraphData> {
    nodes: BTreeMap<&'static str, Box<dyn ErasedGraphNode<Data>>>,
}

impl<Data: GraphData> Clone for GraphNodeRegistry<Data> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
        }
    }
}

impl<Data: GraphData> GraphNodeRegistry<Data> {
    pub fn with_capacity() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    pub fn register<T: ErasedGraphNode<Data> + Default>(&mut self) {
        let node = Box::new(T::default());
        self.nodes.insert(node.name(), node);
    }

    pub fn get(&self, name: &str) -> Option<Box<dyn ErasedGraphNode<Data>>> {
        self.nodes.get(name).cloned()
    }

    pub fn all(&self) -> &BTreeMap<&'static str, Box<dyn ErasedGraphNode<Data>>> {
        &self.nodes
    }

    pub fn merge(&mut self, other: GraphNodeRegistry<Data>) {
        self.nodes.extend(other.nodes);
    }
}
