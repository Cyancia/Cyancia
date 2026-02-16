use cyancia_shader_graph::{
    graph::{
        Graph, GraphCompileError, GraphDynamicInstancesStorage,
        node::{GraphNodeCodeGenContext, GraphNodeCodeGenError, StatelessCommonGraphNode},
        slot::{GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{ColorType, Vec2FType},
};
use glam::Vec4;
use iced_core::{Color, color};
use wesl::{VirtualResolver, Wesl};

pub fn generate_brush_shader(graph: &mut Graph) -> Result<String, GraphCompileError> {
    let template = include_str!("brush_template.wgsl");
    let (_, graph_code) = graph.compile(Vec::new(), Default::default())?;
    let code = template.replace("//CODEGENFLAG_COMPILED_GRAPH", &graph_code);

    // TODO: Use wesl later. Currently it has problem with resolving `binding_array`. Seems to be a bug.
    Ok(code)

    // let mut resolver = VirtualResolver::new();
    // resolver.add_module("template.wesl".parse().unwrap(), code.into());
    // let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
    // compiler.set_mangler(Default::default());
    // compiler.set_options(Default::default());

    // let shader = compiler.compile(&"template.wesl".parse().unwrap()).unwrap();
    // Ok(shader.to_string())
}

pub fn brush_graph_storage() -> GraphDynamicInstancesStorage {
    let mut storage = GraphDynamicInstancesStorage::default();
    storage.nodes.register::<PenPosition>();
    storage.nodes.register::<PixelPosition>();
    storage.nodes.register::<OutputPixelColor>();
    storage
}

#[derive(Default, Clone)]
pub struct PenPosition;

impl StatelessCommonGraphNode for PenPosition {
    fn name(&self) -> &'static str {
        "Pen Position"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Position"]
    }

    fn header_color(&self) -> Color {
        color!(0x79b5f2)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.pen_position;",
            ctx.get_output(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct PixelPosition;

impl StatelessCommonGraphNode for PixelPosition {
    fn name(&self) -> &'static str {
        "Pixel Position"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Position"]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2a0)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = tile_info.tile_index.xy * tile_info.tile_size.xy + id.xy;",
            ctx.get_output(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct OutputPixelColor;

impl StatelessCommonGraphNode for OutputPixelColor {
    fn name(&self) -> &'static str {
        "Output Pixel Color"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2bb)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO)]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "textureStore(output, id.xy, {});\n",
            ctx.get_input(0)?
        ))
    }
}
