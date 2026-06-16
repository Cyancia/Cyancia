use std::fmt;

use bevy_math::{IRect, Rect};
use cyancia_image::blend_modes::BlendMode;
use cyancia_math::{mat3::Mat3ScaleRotattionTranslationWithAnchor, rect_transform::RectTransform};
use cyancia_shader_graph::{
    graph::{
        GraphData,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeRenderContext, GraphNodeRunContext, GraphNodeRunError,
            GraphNodeUpdateSignatureContext, StatelessCommonGraphNode, stateless,
        },
        slot::{GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::{
        nodes::{GraphDataWithTime, GraphTimes},
        types::{ColorType, F32Type, RectType, TextureReference, TextureType, Vec2FType},
    },
};
use cyancia_utils::random_oklch;
use glam::{Mat3, Vec2, Vec4};
use gpui::{AnyElement, App, AppContext, Entity, Rgba, SharedString};
use gpui_component::{
    IndexPath, Sizable,
    searchable_list::SearchableListItem,
    select::{SearchableVec, Select, SelectEvent, SelectState},
};
use serde::{Deserialize, Serialize};

use crate::render::{ComputedPenInput, Time};

#[derive(Clone, Copy, PartialEq, Eq)]
struct BlendModeItem(BlendMode);

impl SearchableListItem for BlendModeItem {
    type Value = BlendMode;

    fn title(&self) -> SharedString {
        self.0.to_string().into()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

#[derive(Default, Clone)]
pub struct BrushGraphPostprocessData {
    pub accumulated_pixel_bounds: IRect,
    pub time: Time,
}

impl GraphData for BrushGraphPostprocessData {}

impl GraphDataWithTime for BrushGraphPostprocessData {
    fn time(&self) -> GraphTimes {
        GraphTimes {
            now: self.time.now,
            stroke_begin: self.time.stroke_begin,
        }
    }

    fn wgsl_variable() -> String {
        "sample.time".into()
    }
}

#[derive(Default, Clone)]
pub struct BrushGraphData {
    pub pen_input: ComputedPenInput,
}

impl GraphData for BrushGraphData {}

impl GraphDataWithTime for BrushGraphData {
    fn time(&self) -> GraphTimes {
        GraphTimes {
            now: self.pen_input.time.now,
            stroke_begin: self.pen_input.time.stroke_begin,
        }
    }

    fn wgsl_variable() -> String {
        "sample.time".into()
    }
}

#[derive(Default, Clone)]
pub struct BrushGraphDataTuple {
    pub lhs: BrushGraphData,
    pub rhs: BrushGraphData,
}

impl GraphData for BrushGraphDataTuple {}

#[derive(Default, Clone)]
pub struct PenPositionNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphData> for PenPositionNode {
    fn name(&self) -> &'static str {
        "Pen Position"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(PenPositionNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>("Position".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphData>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.position;",
            ctx.get_output(0)?
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphData>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<Vec2FType>(0, ctx.data.pen_input.position)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct DrawDirectionNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphData> for DrawDirectionNode {
    fn name(&self) -> &'static str {
        "Draw Direction"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(DrawDirectionNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("Angle".into()),
            GraphDefaultOutputSlot::new::<Vec2FType>("Direction".into()),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphData>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.draw_direction_angle;\nlet {} = graph_input.draw_direction_vec;\n",
            ctx.get_output(0)?,
            ctx.get_output(1)?
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphData>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<F32Type>(0, ctx.data.pen_input.draw_direction_angle)?;
        ctx.set_output_value::<Vec2FType>(1, ctx.data.pen_input.draw_direction_vec)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct PixelPositionNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for PixelPositionNode {
    fn name(&self) -> &'static str {
        "Pixel Position"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(PixelPositionNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>("Position".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("let {} = pixel_posf;", ctx.get_output(0)?))
    }
}

#[derive(Default, Clone)]
pub struct FilterWithinMaskNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for FilterWithinMaskNode {
    fn name(&self) -> &'static str {
        "Filter Within Mask"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(FilterWithinMaskNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color".into(), Vec4::ZERO),
            GraphDefaultInputSlot::new::<TextureType>("Mask".into(), TextureReference::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>("Translation".into(), Vec2::ZERO),
            GraphDefaultInputSlot::new::<F32Type>("Rotation".into(), 0.0),
            GraphDefaultInputSlot::new::<Vec2FType>("Scale".into(), Vec2::ONE),
            GraphDefaultInputSlot::new::<Vec2FType>("Anchor".into(), Vec2::splat(0.5)),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<ColorType>("Color".into()),
            GraphDefaultOutputSlot::new::<RectType>("Bounds".into()),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let mask = ctx.get_input(1)?;
        let translation = ctx.get_input(2)?;
        let rotation = ctx.get_input(3)?;
        let scale = ctx.get_input(4)?;
        let anchor = ctx.get_input(5)?;

        Ok(format!(
            "let {} = filter_within_mask(pixel_pos, {}, {}, {}, {}, {}, {});\nlet {} = filter_within_mask_bounds({}, {}, {}, {}, {});\n",
            ctx.get_output(0)?,
            color,
            mask,
            scale,
            rotation,
            translation,
            anchor,
            ctx.get_output(1)?,
            mask,
            scale,
            rotation,
            translation,
            anchor,
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let mask = ctx.get_input_value::<TextureType>(1)?;
        let translation = ctx.get_input_value::<Vec2FType>(2)?;
        let rotation = ctx.get_input_value::<F32Type>(3)?;
        let scale = ctx.get_input_value::<Vec2FType>(4)?;
        let anchor = ctx.get_input_value::<Vec2FType>(5)?;
        let texture = ctx
            .resources
            .textures
            .get(&mask.external_id)
            .expect("Texture not found");
        let img = &texture.handle.get().unwrap().image;
        let size = Vec2::new(img.width() as f32, img.height() as f32);
        let bounds = Rect {
            min: Vec2::ZERO,
            max: size,
        };

        let mat = Mat3::from_scale_angle_translation_with_anchor(
            scale,
            rotation,
            translation,
            anchor * size,
        );
        let bounds = bounds.transformed(&mat);

        ctx.set_output_value::<RectType>(1, bounds)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct FilterWithinBoundsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for FilterWithinBoundsNode {
    fn name(&self) -> &'static str {
        "Filter Within Bounds"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(FilterWithinBoundsNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color".into(), Vec4::ZERO),
            GraphDefaultInputSlot::new::<RectType>("Bounds".into(), Rect::default()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<ColorType>("Color".into()),
            GraphDefaultOutputSlot::new::<RectType>("Bounds".into()),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let bounds = ctx.get_input(1)?;

        Ok(format!(
            "let {} = filter_within_bounds(pixel_pos, {}, {});\nlet {} = {};\n",
            ctx.get_output(0)?,
            color,
            bounds,
            ctx.get_output(1)?,
            bounds
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let bounds = ctx.get_input_value::<RectType>(1)?;
        ctx.set_output_value::<ColorType>(0, Vec4::ZERO)?;
        ctx.set_output_value::<RectType>(1, bounds)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct OutputColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputColorNode {
    fn name(&self) -> &'static str {
        "Output Color"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(OutputColorNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(
            "Color".into(),
            Vec4::ZERO,
        )]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "set_output_color(pixel_pos, {});\n",
            ctx.get_input(0)?
        ))
    }

    fn run(&self, _: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct OutputBoundsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputBoundsNode {
    fn name(&self) -> &'static str {
        "Output Bounds"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(OutputBoundsNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<RectType>(
            "Bounds".into(),
            Rect::default(),
        )]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn update_signature(&self, mut ctx: GraphNodeUpdateSignatureContext<'_, Data>) {
        ctx.require_input_slot_as_graph_output(0, "Bounds".to_string());
    }

    fn generate_code(
        &self,
        _: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(Default::default())
    }

    fn run(&self, _: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct PasteTextureNode;

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteTextureMode {
    Clamp,
    #[default]
    Wrap,
}

impl PasteTextureMode {
    const ALL: [PasteTextureMode; 2] = [PasteTextureMode::Clamp, PasteTextureMode::Wrap];
}

impl SearchableListItem for PasteTextureMode {
    type Value = Self;

    fn title(&self) -> SharedString {
        self.to_string().into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

impl fmt::Display for PasteTextureMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasteTextureMode::Clamp => f.write_str("Clamp"),
            PasteTextureMode::Wrap => f.write_str("Wrap"),
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct PasteTextureNodeState {
    pub mode: PasteTextureMode,
}

impl<Data: GraphData> GraphNode<Data> for PasteTextureNode {
    type State = PasteTextureNodeState;

    fn name(&self) -> &'static str {
        "Paste Texture"
    }

    fn default_state(&self) -> Self::State {
        Default::default()
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(PasteTextureNode, cx)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>("Texture".into(), TextureReference::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>("Translation".into(), Vec2::ZERO),
            GraphDefaultInputSlot::new::<F32Type>("Rotation".into(), 0.0),
            GraphDefaultInputSlot::new::<Vec2FType>("Scale".into(), Vec2::ONE),
            GraphDefaultInputSlot::new::<Vec2FType>("Anchor".into(), Vec2::splat(0.5)),
        ]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(PasteTextureMode::ALL),
                        PasteTextureMode::ALL
                            .iter()
                            .position(|mode| mode == &state.mode)
                            .map(IndexPath::new),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state_entity,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let _ = graph.update(cx, |graph, cx| {
                                graph.update_node_state::<PasteTextureNode>(
                                    cx,
                                    node_id,
                                    move |state| {
                                        state.mode = *val;
                                    },
                                );
                            });
                        }
                    },
                )
                .detach();

                state_entity
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let tex = ctx.get_input(0)?;
        let translation = ctx.get_input(1)?;
        let rotation = ctx.get_input(2)?;
        let scale = ctx.get_input(3)?;
        let anchor = ctx.get_input(4)?;
        let output = ctx.get_output(0)?;
        let fn_name = match state.mode {
            PasteTextureMode::Clamp => "sample_transformed_local_texture_clamp",
            PasteTextureMode::Wrap => "sample_transformed_local_texture_wrap",
        };
        Ok(format!(
            "let {} = {}({}, pixel_posf, {}, {}, {}, {});\n",
            output, fn_name, tex, scale, rotation, translation, anchor
        ))
    }
}

#[derive(Default, Clone)]
pub struct CurrentPixelColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for CurrentPixelColorNode {
    fn name(&self) -> &'static str {
        "Current Pixel Color"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(CurrentPixelColorNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(
            "Position".into(),
            Vec2::ZERO,
        )]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = current_input_color(vec2i({}));\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct LayerPixelColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for LayerPixelColorNode {
    fn name(&self) -> &'static str {
        "Layer Pixel Color"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(LayerPixelColorNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(
            "Position".into(),
            Vec2::ZERO,
        )]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = target_layer_color(vec2i({}));\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct BlendColorNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendColorNodeState {
    pub blend_mode: BlendMode,
}

impl<Data: GraphData> GraphNode<Data> for BlendColorNode {
    type State = BlendColorNodeState;

    fn name(&self) -> &'static str {
        "Blend Color"
    }

    fn default_state(&self) -> Self::State {
        BlendColorNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(BlendColorNode, cx)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Src Color".into(), Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>("Dst Color".into(), Vec4::ZERO),
        ]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let items = BlendMode::ALL.map(BlendModeItem);
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(items),
                        BlendMode::ALL
                            .iter()
                            .position(|mode| mode == &state.blend_mode)
                            .map(IndexPath::new),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state_entity,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let _ = graph.update(cx, |graph, cx| {
                                graph.update_node_state::<BlendColorNode>(
                                    cx,
                                    node_id,
                                    move |state| {
                                        state.blend_mode = *val;
                                    },
                                );
                            });
                        }
                    },
                )
                .detach();

                state_entity
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let src = ctx.get_input(0)?;
        let dst = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;
        Ok(format!(
            "let {} = package::image::blend_modes::{}({}, {});\n",
            output,
            state.blend_mode.shader_func(),
            src,
            dst
        ))
    }
}

#[derive(Default, Clone)]
pub struct StrokeBoundsNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphPostprocessData> for StrokeBoundsNode {
    fn name(&self) -> &'static str {
        "Stroke Bounds"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(StrokeBoundsNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphPostprocessData>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphPostprocessData>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<RectType>("Bounds".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphPostprocessData>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = Rect(vec2f(sample.accumulated_bound.min), vec2f(sample.accumulated_bound.max));",
            ctx.get_output(0)?
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphPostprocessData>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<RectType>(
            0,
            Rect {
                min: ctx.data.accumulated_pixel_bounds.min.as_vec2(),
                max: ctx.data.accumulated_pixel_bounds.max.as_vec2(),
            },
        )?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct EllipticalMaskNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for EllipticalMaskNode {
    fn name(&self) -> &'static str {
        "Elliptical Mask"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(EllipticalMaskNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<Vec2FType>("Sample Position".into(), Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>("Center".into(), Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>("Radii".into(), Vec2::ZERO),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("Mask Value".into()),
            GraphDefaultOutputSlot::new::<RectType>("Bounds".into()),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let mask = ctx.ident_generator.next_output();
        Ok(format!(
            "let {mask} = elliptical_mask({}, {}, {});\nlet {} = {mask}.value;\nlet {} = {mask}.bounds;\n",
            ctx.get_input(0)?,
            ctx.get_input(1)?,
            ctx.get_input(2)?,
            ctx.get_output(0)?,
            ctx.get_output(1)?
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let center = ctx.get_input_value::<Vec2FType>(1)?;
        let radii = ctx.get_input_value::<Vec2FType>(2)?;

        // Mask value in slot 0 is not evaluated on cpu.
        ctx.set_output_value::<RectType>(
            1,
            Rect {
                min: center - radii,
                max: center + radii,
            },
        )?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct BlendWithInputNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendWithBufferNodeState {
    pub blend_mode: BlendMode,
}

impl<Data: GraphData> GraphNode<Data> for BlendWithInputNode {
    type State = BlendWithBufferNodeState;

    fn name(&self) -> &'static str {
        "Blend With Input"
    }

    fn default_state(&self) -> Self::State {
        BlendWithBufferNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(BlendWithInputNode, cx)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color".into(), Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>("Opacity".into(), 1.0),
        ]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let items = BlendMode::ALL.map(BlendModeItem);
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(items),
                        BlendMode::ALL
                            .iter()
                            .position(|mode| mode == &state.blend_mode)
                            .map(IndexPath::new),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state_entity,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let _ = graph.update(cx, |graph, cx| {
                                graph.update_node_state::<BlendColorNode>(
                                    cx,
                                    node_id,
                                    move |state| {
                                        state.blend_mode = *val;
                                    },
                                );
                            });
                        }
                    },
                )
                .detach();

                state_entity
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let opacity = ctx.get_input(1)?;
        Ok(format!(
            "let {} = package::image::blend_modes::{}(vec4f({color}.rgb, {color}.a * {opacity}), current_input_color(pixel_pos));\n",
            ctx.get_output(0)?,
            state.blend_mode.shader_func()
        ))
    }
}

#[derive(Default, Clone)]
pub struct BlendWithLayerNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendWithLayerNodeState {
    pub blend_mode: BlendMode,
}

impl<Data: GraphData> GraphNode<Data> for BlendWithLayerNode {
    type State = BlendWithLayerNodeState;

    fn name(&self) -> &'static str {
        "Blend With Layer"
    }

    fn default_state(&self) -> Self::State {
        BlendWithLayerNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(BlendWithLayerNode, cx)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color".into(), Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>("Opacity".into(), 1.0),
        ]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let items = BlendMode::ALL.map(BlendModeItem);
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(items),
                        BlendMode::ALL
                            .iter()
                            .position(|mode| mode == &state.blend_mode)
                            .map(IndexPath::new),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state_entity,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let _ = graph.update(cx, |graph, cx| {
                                graph.update_node_state::<BlendColorNode>(
                                    cx,
                                    node_id,
                                    move |state| {
                                        state.blend_mode = *val;
                                    },
                                );
                            });
                        }
                    },
                )
                .detach();

                state_entity
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let opacity = ctx.get_input(1)?;
        Ok(format!(
            "let {} = package::image::blend_modes::{}(vec4f({color}.rgb, {color}.a * {opacity}), target_layer_color(pixel_pos));\n",
            ctx.get_output(0)?,
            state.blend_mode.shader_func()
        ))
    }
}

#[derive(Default, Clone)]
pub struct OutputSpacingNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputSpacingNode {
    fn name(&self) -> &'static str {
        "Output Spacing"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(OutputSpacingNode, cx)
    }

    fn update_signature(&self, mut ctx: GraphNodeUpdateSignatureContext<'_, Data>) {
        ctx.require_input_slot_as_graph_output(0, "Spacing".to_string());
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>("Spacing".into(), 0.0)]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("return {};\n", ctx.get_input(0)?))
    }

    fn run(&self, _: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct PenPositionsNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphDataTuple> for PenPositionsNode {
    fn name(&self) -> &'static str {
        "Pen Positions"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(PenPositionsNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<Vec2FType>("Src Position".into()),
            GraphDefaultOutputSlot::new::<Vec2FType>("Dst Position".into()),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphDataTuple>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "
            let {} = src.position;
            let {} = dst.position;
            ",
            ctx.get_output(0)?,
            ctx.get_output(1)?,
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphDataTuple>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<Vec2FType>(0, ctx.data.lhs.pen_input.position)?;
        ctx.set_output_value::<Vec2FType>(1, ctx.data.rhs.pen_input.position)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct DrawDirectionsNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphDataTuple> for DrawDirectionsNode {
    fn name(&self) -> &'static str {
        "Draw Directions"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(DrawDirectionsNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("Src Angle".into()),
            GraphDefaultOutputSlot::new::<F32Type>("Dst Angle".into()),
            GraphDefaultOutputSlot::new::<Vec2FType>("Src Direction".into()),
            GraphDefaultOutputSlot::new::<Vec2FType>("Dst Direction".into()),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphDataTuple>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "
            let {} = src.draw_direction_angle;
            let {} = dst.draw_direction_angle;
            let {} = src.draw_direction_vec;
            let {} = dst.draw_direction_vec;
            ",
            ctx.get_output(0)?,
            ctx.get_output(1)?,
            ctx.get_output(2)?,
            ctx.get_output(3)?,
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphDataTuple>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<F32Type>(0, ctx.data.lhs.pen_input.draw_direction_angle)?;
        ctx.set_output_value::<F32Type>(1, ctx.data.rhs.pen_input.draw_direction_angle)?;
        ctx.set_output_value::<Vec2FType>(2, ctx.data.lhs.pen_input.draw_direction_vec)?;
        ctx.set_output_value::<Vec2FType>(3, ctx.data.rhs.pen_input.draw_direction_vec)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct TimesNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphDataTuple> for TimesNode {
    fn name(&self) -> &'static str {
        "Times"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(TimesNode, cx)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("Src Now".into()),
            GraphDefaultOutputSlot::new::<F32Type>("Src Stroke Begin".into()),
            GraphDefaultOutputSlot::new::<F32Type>("Dst Now".into()),
            GraphDefaultOutputSlot::new::<F32Type>("Dst Stroke Begin".into()),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphDataTuple>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "
            let {} = src.time.now;
            let {} = src.time.stroke_begin;
            let {} = dst.time.now;
            let {} = dst.time.stroke_begin;
            ",
            ctx.get_output(0)?,
            ctx.get_output(1)?,
            ctx.get_output(2)?,
            ctx.get_output(3)?,
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphDataTuple>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<F32Type>(0, ctx.data.lhs.pen_input.time.now)?;
        ctx.set_output_value::<F32Type>(1, ctx.data.lhs.pen_input.time.stroke_begin)?;
        ctx.set_output_value::<F32Type>(2, ctx.data.rhs.pen_input.time.now)?;
        ctx.set_output_value::<F32Type>(3, ctx.data.rhs.pen_input.time.stroke_begin)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct OutputRequiredSpacingNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputRequiredSpacingNode {
    fn name(&self) -> &'static str {
        "Output Required Spacing"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(OutputRequiredSpacingNode, cx)
    }

    fn update_signature(&self, mut ctx: GraphNodeUpdateSignatureContext<'_, Data>) {
        ctx.require_input_slot_as_graph_output(0, "Required Spacing".to_string());
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(
            "Required Spacing".into(),
            0.0,
        )]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("return {};\n", ctx.get_input(0)?))
    }

    fn run(&self, _: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}
