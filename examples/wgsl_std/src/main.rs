use std::sync::Arc;

use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    editor::{GraphView, GraphViewMessage},
    graph::{
        Graph, GraphFunctionsStorage, GraphSignature,
        node::{
            GraphNodeCodeGenContext, GraphNodeCodeGenError, StatelessCommonGraphNode,
            external::{ExternalDataStorage, ExternalLiteralId, ExternalNode},
            function::{GraphFunctionId, GraphFunctionNode, functioning},
        },
        slot::{GraphDefaultInputSlot, GraphDefaultOutputSlot},
        variable::GraphLiteral,
    },
    wgsl_std::{
        self,
        types::{F32Type, Vec2FType},
    },
};
use glam::Vec2;
use iced::{
    Color, Element, Subscription, color,
    keyboard::{self, key},
    widget::row,
};

fn main() {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .run()
        .unwrap();
}

pub struct App {
    graph: Graph,
    functions: Arc<GraphFunctionsStorage>,
}

#[derive(Debug)]
pub enum GraphMessage {
    Keyboard(keyboard::Event),
    View(GraphViewMessage),
}

impl App {
    pub fn new() -> Self {
        let mut storage = wgsl_std::std_storage();
        let ext_storage = ExternalDataStorage::default();
        ext_storage.insert(
            ExternalLiteralId::new("MyExternalF32".into()),
            GraphLiteral::new::<F32Type>(0.0),
        );
        ext_storage.insert(
            ExternalLiteralId::new("MyExternalVec2F".into()),
            GraphLiteral::new::<Vec2FType>(Vec2::ZERO),
        );
        let functions = Arc::new(GraphFunctionsStorage::default());
        storage
            .nodes
            .register_non_default(ExternalNode::new(ext_storage.into()));
        storage
            .nodes
            .register_non_default(GraphFunctionNode::new(functions.clone()));
        storage.nodes.register_non_default(DummyOutputNode);
        storage.merge(functioning());
        Self {
            graph: Graph::new(storage.into()),
            functions,
        }
    }

    // pub fn view(&self) -> Element<'_, GraphEditorMessage<GraphMessage>> {
    pub fn view(&self) -> Element<'_, GraphMessage, GraphTheme, GraphRenderer> {
        row![Element::new(GraphView::new(&self.graph)).map(GraphMessage::View)].into()
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
                        let Some(path) = rfd::FileDialog::new().save_file() else {
                            return;
                        };

                        let Ok(toml) = self.graph.to_toml() else {
                            return;
                        };
                        std::fs::write(path, toml).unwrap();
                    }
                    if physical_key == key::Physical::Code(key::Code::KeyO) {
                        let Some(path) = rfd::FileDialog::new().pick_file() else {
                            return;
                        };
                        let s = std::fs::read_to_string(path).unwrap();
                        let (graph, errors) = Graph::from_toml(self.graph.storage().clone(), &s);
                        for error in errors {
                            println!("Deserialization error: {}", error);
                        }
                        if let Some(graph) = graph {
                            self.graph = graph;
                        }
                    }
                    if physical_key == key::Physical::Code(key::Code::KeyL) {
                        let Some(path) = rfd::FileDialog::new().pick_file() else {
                            return;
                        };
                        let s = std::fs::read_to_string(&path).unwrap();
                        let (graph, errors) = Graph::from_toml(self.graph.storage().clone(), &s);
                        for error in errors {
                            println!("Deserialization error: {}", error);
                        }
                        if let Some(graph) = graph {
                            self.functions.insert(
                                GraphFunctionId {
                                    name: path.file_stem().unwrap().to_string_lossy().to_string(),
                                },
                                graph,
                            );
                            println!(
                                "Function loaded: {}",
                                path.file_stem().unwrap().to_string_lossy()
                            );
                            dbg!("Current functions: {:?}", self.functions.all().keys());
                        }
                    }
                }
            }
            _ => {}
        }

        match self.graph.compile(Default::default(), Default::default()) {
            Ok((_, code)) => println!("{}", code),
            Err(e) => println!("Code generation failed: {}", e),
        }
    }

    pub fn subscription(&self) -> Subscription<GraphMessage> {
        keyboard::listen().map(GraphMessage::Keyboard)
    }
}

#[derive(Default, Clone)]
pub struct DummyOutputNode;

impl StatelessCommonGraphNode for DummyOutputNode {
    fn name(&self) -> &'static str {
        "Dummy Output"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Input"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn header_color(&self) -> Color {
        color!(0x79bdf2)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input = ctx.get_input(0)?;
        Ok(format!("return {};\n", input))
    }
}
