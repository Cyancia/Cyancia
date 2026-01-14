use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    editor::{GraphView, GraphViewMessage},
    graph::{
        Graph, GraphSignature,
        node::{GraphNodeCodeGenContext, GraphNodeCodeGenError, StatelessCommonGraphNode},
        slot::{GraphDefaultInputSlot, GraphDefaultOutputSlot},
        variable::GraphLiteral,
    },
    wgsl_std::{
        self,
        nodes::external::{ExternalDataStorage, ExternalLiteralId, ExternalNode},
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
}

#[derive(Debug)]
pub enum GraphMessage {
    Keyboard(keyboard::Event),
    View(GraphViewMessage),
}

impl App {
    pub fn new() -> Self {
        let mut storage = wgsl_std::create_storage();
        let ext_storage = ExternalDataStorage::default();
        ext_storage.insert(
            ExternalLiteralId::new("MyExternalF32".into()),
            GraphLiteral::new::<F32Type>(0.0),
        );
        ext_storage.insert(
            ExternalLiteralId::new("MyExternalVec2F".into()),
            GraphLiteral::new::<Vec2FType>(Vec2::ZERO),
        );
        storage
            .nodes
            .register_non_default(ExternalNode::new(ext_storage.into()));
        storage.nodes.register_non_default(DummyOutputNode);
        Self {
            graph: Graph::new("testtt".into(), storage.into()),
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

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let input = ctx.get_input(0)?;
        Ok(format!("return {};\n", input))
    }
}
