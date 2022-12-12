use self::{
    attachments::{AttachmentsNode, AttachmentsPlugin},
    compute::{
        animation::AnimationNode, automata::AutomataNode, clear::ClearNode, physics::PhysicsNode,
        rebuild::RebuildNode, ComputeResourcesPlugin,
    },
    denoise::{DenoiseNode, DenoisePlugin},
    trace::{TraceNode, TracePlugin},
    voxel_world::VoxelWorldPlugin,
    voxelization::VoxelizationPlugin,
};
use bevy::{
    core_pipeline::{
        bloom::BloomNode, fxaa::FxaaNode, tonemapping::TonemappingNode, upscaling::UpscalingNode,
    },
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        main_graph::node::CAMERA_DRIVER,
        render_graph::{RenderGraph, SlotInfo, SlotType},
        RenderApp,
    },
    ui::UiPassNode,
};

pub mod attachments;
pub mod compute;
pub mod denoise;
pub mod trace;
pub mod voxel_world;
pub mod voxelization;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RenderGraphSettings::default())
            .add_plugin(ExtractResourcePlugin::<RenderGraphSettings>::default())
            .add_plugin(AttachmentsPlugin)
            .add_plugin(VoxelWorldPlugin)
            .add_plugin(TracePlugin)
            .add_plugin(VoxelizationPlugin)
            .add_plugin(ComputeResourcesPlugin)
            .add_plugin(DenoisePlugin);

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        // build voxel render graph
        let mut voxel_graph = RenderGraph::default();
        let input_node_id =
            voxel_graph.set_input(vec![SlotInfo::new("view_entity", SlotType::Entity)]);

        // voxel render graph
        let attachments = AttachmentsNode::new(&mut render_app.world);
        let trace = TraceNode::new(&mut render_app.world);
        let denoise = DenoiseNode::new(&mut render_app.world);
        let bloom = BloomNode::new(&mut render_app.world);
        let tonemapping = TonemappingNode::new(&mut render_app.world);
        let fxaa = FxaaNode::new(&mut render_app.world);
        let ui = UiPassNode::new(&mut render_app.world);
        let upscaling = UpscalingNode::new(&mut render_app.world);

        voxel_graph.add_node("attachments", attachments);
        voxel_graph.add_node("trace", trace);
        voxel_graph.add_node("denoise", denoise);
        voxel_graph.add_node("bloom", bloom);
        voxel_graph.add_node("tonemapping", tonemapping);
        voxel_graph.add_node("fxaa", fxaa);
        voxel_graph.add_node("ui", ui);
        voxel_graph.add_node("upscaling", upscaling);
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "attachments", "view")
            .unwrap();
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "trace", "view")
            .unwrap();
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "denoise", "view")
            .unwrap();
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "bloom", "view")
            .unwrap();
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "tonemapping", "view")
            .unwrap();
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "fxaa", "view")
            .unwrap();
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "ui", "view")
            .unwrap();
        voxel_graph
            .add_slot_edge(input_node_id, "view_entity", "upscaling", "view")
            .unwrap();
        voxel_graph.add_node_edge("trace", "denoise").unwrap();
        voxel_graph.add_node_edge("denoise", "bloom").unwrap();
        voxel_graph.add_node_edge("bloom", "tonemapping").unwrap();
        voxel_graph.add_node_edge("tonemapping", "fxaa").unwrap();
        voxel_graph.add_node_edge("fxaa", "ui").unwrap();
        voxel_graph.add_node_edge("ui", "upscaling").unwrap();
        voxel_graph
            .add_slot_edge("attachments", "accumulation", "trace", "accumulation")
            .unwrap();
        voxel_graph
            .add_slot_edge("attachments", "normal", "trace", "normal")
            .unwrap();
        voxel_graph
            .add_slot_edge("attachments", "position", "trace", "position")
            .unwrap();
        voxel_graph
            .add_slot_edge("attachments", "accumulation", "denoise", "accumulation")
            .unwrap();
        voxel_graph
            .add_slot_edge("attachments", "normal", "denoise", "normal")
            .unwrap();
        voxel_graph
            .add_slot_edge("attachments", "position", "denoise", "position")
            .unwrap();

        // voxel render graph compute
        let rebuild = RebuildNode;
        let physics = PhysicsNode;
        voxel_graph.add_node("rebuild", rebuild);
        voxel_graph.add_node("physics", physics);
        voxel_graph.add_node_edge("rebuild", "physics").unwrap();
        voxel_graph.add_node_edge("physics", "trace").unwrap();

        // main graph compute
        let mut graph = render_app.world.resource_mut::<RenderGraph>();
        let clear = ClearNode;
        let automata = AutomataNode;
        let animation = AnimationNode;
        graph.add_node("clear", clear);
        graph.add_node("automata", automata);
        graph.add_node("animation", animation);
        graph.add_node_edge("clear", "automata").unwrap();
        graph.add_node_edge("automata", "animation").unwrap();
        graph.add_node_edge("animation", CAMERA_DRIVER).unwrap();

        // insert the voxel graph into the main render graph
        graph.add_sub_graph("voxel", voxel_graph);
    }
}

#[derive(Resource, Clone, ExtractResource)]
pub struct RenderGraphSettings {
    pub clear: bool,
    pub automata: bool,
    pub animation: bool,
    pub voxelization: bool,
    pub rebuild: bool,
    pub physics: bool,
    pub trace: bool,
    pub denoise: bool,
}

impl Default for RenderGraphSettings {
    fn default() -> Self {
        Self {
            clear: true,
            automata: true,
            animation: true,
            voxelization: true,
            rebuild: true,
            physics: true,
            trace: true,
            denoise: false,
        }
    }
}