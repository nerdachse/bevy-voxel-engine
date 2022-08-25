use super::{load::GH, trace, trace::ExtractedPortal};
use bevy::{
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_graph::{self, RenderGraph},
        render_resource::*,
        renderer::RenderQueue,
        renderer::{RenderContext, RenderDevice},
        RenderApp, RenderStage,
    },
};
use std::{borrow::Cow, collections::HashMap};

const MAX_ANIMATION_DATA: usize = 1024000;

pub struct ComputePlugin;

impl Plugin for ComputePlugin {
    fn build(&self, app: &mut App) {
        let render_device = app.world.resource::<RenderDevice>();

        // compute data buffer
        let physics_data = render_device.create_buffer_with_data(&BufferInitDescriptor {
            contents: bytemuck::cast_slice(&vec![0u32; MAX_ANIMATION_DATA]),
            label: None,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        });

        // compute data buffer
        let animation_data = render_device.create_buffer_with_data(&BufferInitDescriptor {
            contents: bytemuck::cast_slice(&vec![0u32; MAX_ANIMATION_DATA]),
            label: None,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        // setup world
        app.add_system(extract_animation_data)
            .add_system(extract_physics_data)
            .insert_resource(ComputeMeta {
                physics_data,
                animation_data,
            })
            .insert_resource(ExtractedPhysicsData {
                data: Vec::new(),
                entities: HashMap::new(),
            })
            .add_plugin(ExtractResourcePlugin::<ExtractedGH>::default())
            .add_plugin(ExtractResourcePlugin::<ExtractedAnimationData>::default())
            .add_plugin(ExtractResourcePlugin::<ExtractedPhysicsData>::default())
            .add_plugin(ExtractResourcePlugin::<ComputeMeta>::default());

        // setup render world
        app.sub_app_mut(RenderApp)
            .init_resource::<ComputePipeline>()
            .add_system_to_stage(RenderStage::Queue, queue_bind_group);

        // setup render graph
        let render_app = app.sub_app_mut(RenderApp);
        let mut render_graph = render_app.world.resource_mut::<RenderGraph>();
        render_graph.add_node("compute", ComputeNode::default());
        render_graph
            .add_node_edge("compute", bevy::render::main_graph::node::CAMERA_DRIVER)
            .unwrap();
    }
}

#[derive(Component)]
pub struct Particle {
    pub material: u8,
}

/// normal must be a normalized voxel normal
#[derive(Component)]
pub struct Portal {
    pub material: u8,
    pub half_size: IVec3,
    pub normal: Vec3,
}

#[derive(Component)]
pub struct Edges {
    pub material: u8,
    pub half_size: IVec3,
}

#[derive(Component)]
pub struct Bullet {
    pub velocity: Vec3,
}

#[derive(Clone, ExtractResource)]
struct ExtractedAnimationData {
    data: Vec<u32>,
}

#[derive(Clone, ExtractResource)]
struct ExtractedPhysicsData {
    data: Vec<u32>,
    entities: HashMap<Entity, usize>,
}

#[derive(Clone)]
struct TypeBuffer {
    header: Vec<u32>,
    data: Vec<u32>,
}

impl TypeBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            header: Vec::new(),
        }
    }

    fn finish(mut self) -> Vec<u32> {
        // move all the pointers based on the header length
        let offset = self.header.len() + 1;
        for i in 0..self.header.len() {
            self.header[i] += offset as u32;
        }

        // combine the header and animation data
        let mut data = vec![self.header.len() as u32];
        data.extend(self.header);
        data.extend(self.data);

        return data;
    }

    fn push_object<F>(&mut self, object_type: u32, function: F)
    where
        // The closure takes an `i32` and returns an `i32`.
        F: Fn(&mut Self),
    {
        self.header
            .push(self.data.len() as u32 | (object_type << 24));
        function(self);
    }

    fn push_u32(&mut self, value: u32) {
        self.data.push(bytemuck::cast(value));
    }

    fn push_vec3(&mut self, value: Vec3) {
        self.data.push(bytemuck::cast(value.x));
        self.data.push(bytemuck::cast(value.y));
        self.data.push(bytemuck::cast(value.z));
    }

    fn push_ivec3(&mut self, value: IVec3) {
        self.data.push(bytemuck::cast(value.x));
        self.data.push(bytemuck::cast(value.y));
        self.data.push(bytemuck::cast(value.z));
    }
}

const VOXELS_PER_METER: u32 = 4;

pub fn world_to_voxel(world_pos: Vec3, voxel_world_size: u32) -> IVec3 {
    let world_pos = world_pos * VOXELS_PER_METER as f32;
    world_pos.as_ivec3() + IVec3::splat(voxel_world_size as i32 / 2)
}

pub fn world_to_render(world_pos: Vec3, voxel_world_size: u32) -> Vec3 {
    2.0 * world_pos * VOXELS_PER_METER as f32 / voxel_world_size as f32
}

fn extract_animation_data(
    mut commands: Commands,
    particle_query: Query<(&Transform, &Particle)>,
    portal_query: Query<(&Transform, &Portal)>,
    edges_query: Query<(&Transform, &Edges)>,
    mut uniforms: ResMut<trace::Uniforms>,
) {
    let mut type_buffer = TypeBuffer::new();

    let voxel_world_size = uniforms.texture_size;

    // add particles
    for (transform, particle) in particle_query.iter() {
        let pos = world_to_voxel(transform.translation, voxel_world_size);
        type_buffer.push_object(0, |type_buffer| {
            type_buffer.push_u32(particle.material as u32);
            type_buffer.push_ivec3(pos);
        });
    }

    // add portals
    let mut i = 0;
    for (transform, portal) in portal_query.iter() {
        let pos = world_to_voxel(transform.translation, voxel_world_size);
        type_buffer.push_object(1, |type_buffer| {
            type_buffer.push_u32(portal.material as u32);
            type_buffer.push_ivec3(pos);
            type_buffer.push_u32(i);
            type_buffer.push_ivec3(portal.half_size);
        });
        i += 1;
    }

    // add edges
    for (transform, edges) in edges_query.iter() {
        let pos = world_to_voxel(transform.translation, voxel_world_size);
        type_buffer.push_object(2, |type_buffer| {
            type_buffer.push_u32(edges.material as u32);
            type_buffer.push_ivec3(pos);
            type_buffer.push_ivec3(edges.half_size);
        });
    }

    // grab all the poratls in pairs
    uniforms.portals = [ExtractedPortal::default(); 32];
    let mut i = 0;
    let mut first: Option<(&Transform, &Portal)> = None;
    for (transform, portal) in portal_query.iter() {
        if i % 2 == 1 {
            let first = first.unwrap();
            let second = (transform, portal);

            let first_normal = first.1.normal;
            let second_normal = second.1.normal;

            let voxel_size = 2.0 / uniforms.texture_size as f32;
            let first_pos =
                world_to_render(first.0.translation, uniforms.texture_size) + voxel_size / 2.0;
            let second_pos =
                world_to_render(second.0.translation, uniforms.texture_size) + voxel_size / 2.0;

            uniforms.portals[i - 1] = ExtractedPortal {
                pos: [first_pos.x, first_pos.y, first_pos.z, 0.0],
                other_pos: [second_pos.x, second_pos.y, second_pos.z, 0.0],
                normal: [first_normal.x, first_normal.y, first_normal.z, 0.0],
                other_normal: [second_normal.x, second_normal.y, second_normal.z, 0.0],
                half_size: [
                    first.1.half_size.x,
                    first.1.half_size.y,
                    first.1.half_size.z,
                    0,
                ],
            };
            uniforms.portals[i] = ExtractedPortal {
                pos: [second_pos.x, second_pos.y, second_pos.z, 0.0],
                other_pos: [first_pos.x, first_pos.y, first_pos.z, 0.0],
                normal: [second_normal.x, second_normal.y, second_normal.z, 0.0],
                other_normal: [first_normal.x, first_normal.y, first_normal.z, 0.0],
                half_size: [
                    second.1.half_size.x,
                    second.1.half_size.y,
                    second.1.half_size.z,
                    0,
                ],
            };
        }
        first = Some((transform, portal));
        i += 1;
    }

    commands.insert_resource(ExtractedAnimationData {
        data: type_buffer.finish(),
    });
}

fn extract_physics_data(
    mut bullet_query: Query<(&mut Transform, &mut Bullet, Entity)>,
    mut extracted_physics_data: ResMut<ExtractedPhysicsData>,
    compute_meta: Res<ComputeMeta>,
    render_device: Res<RenderDevice>,
) {
    // process last frames physics data
    if extracted_physics_data.data.len() > 1 {
        let buffer_slice = compute_meta
            .physics_data
            .slice(..extracted_physics_data.data.len() as u64 * 4);

        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});

        render_device.poll(wgpu::Maintain::Wait);

        let data = buffer_slice.get_mapped_range();
        let result: Vec<u32> = bytemuck::cast_slice(&data).to_vec();

        drop(data);
        compute_meta.physics_data.unmap();

        for (mut transform, mut bullet, entity) in bullet_query.iter_mut() {
            if let Some(index) = extracted_physics_data.entities.get(&entity) {
                let data_index = result[index + 1] as usize;
                transform.translation = Vec3::new(
                    bytemuck::cast(result[data_index + 0]),
                    bytemuck::cast(result[data_index + 1]),
                    bytemuck::cast(result[data_index + 2]),
                );
                bullet.velocity = Vec3::new(
                    bytemuck::cast(result[data_index + 3]),
                    bytemuck::cast(result[data_index + 4]),
                    bytemuck::cast(result[data_index + 5]),
                );
            }
        }
    }

    let mut type_buffer = TypeBuffer::new();
    let mut entities = HashMap::new();

    // add bullets
    for (transform, bullet, entity) in bullet_query.iter() {
        entities.insert(entity, type_buffer.header.len());

        type_buffer.push_object(0, |type_buffer| {
            type_buffer.push_vec3(transform.translation);
            type_buffer.push_vec3(bullet.velocity);
        });
    }

    extracted_physics_data.data = type_buffer.finish();
    extracted_physics_data.entities = entities;
}

#[derive(Clone, ExtractResource)]
struct ComputeMeta {
    physics_data: Buffer,
    animation_data: Buffer,
}

struct ExtractedGH {
    pub buffer_size: usize,
    pub texture_size: u32,
}

impl ExtractResource for ExtractedGH {
    type Source = GH;

    fn extract_resource(gh: &Self::Source) -> Self {
        ExtractedGH {
            buffer_size: gh.get_final_length() as usize / 8,
            texture_size: gh.texture_size,
        }
    }
}

#[derive(PartialEq, Eq)]
enum ComputeState {
    Loading,
    Init,
    Update,
}

struct ComputeNode {
    state: ComputeState,
}

impl Default for ComputeNode {
    fn default() -> Self {
        Self {
            state: ComputeState::Loading,
        }
    }
}

impl render_graph::Node for ComputeNode {
    fn update(&mut self, world: &mut World) {
        let pipeline = world.resource::<ComputePipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        // if the corresponding pipeline has loaded, transition to the next stage
        match self.state {
            ComputeState::Loading => {
                if let CachedPipelineState::Ok(_) =
                    pipeline_cache.get_compute_pipeline_state(pipeline.update_pipeline)
                {
                    if let CachedPipelineState::Ok(_) =
                        pipeline_cache.get_compute_pipeline_state(pipeline.rebuild_pipeline)
                    {
                        self.state = ComputeState::Init;
                    }
                }
            }
            ComputeState::Init => {
                self.state = ComputeState::Update;
            }
            ComputeState::Update => {}
        }
    }

    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        let texture_bind_group = &world.resource::<ComputeImageBindGroup>().0;
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<ComputePipeline>();
        let render_queue = world.resource::<RenderQueue>();
        // let render_device = world.resource::<RenderDevice>();
        let trace_meta = world.resource::<trace::TraceMeta>();
        let compute_meta = world.resource::<ComputeMeta>();
        let extracted_gh = world.resource::<ExtractedGH>();
        let extracted_animation_data = world.resource::<ExtractedAnimationData>();
        let extracted_physics_data = world.resource::<ExtractedPhysicsData>();
        let uniforms = world.resource::<trace::ExtractedUniforms>();

        let mut pass = render_context
            .command_encoder
            .begin_compute_pass(&ComputePassDescriptor::default());

        pass.set_bind_group(0, texture_bind_group, &[]);

        // select the pipeline based on the current state
        match self.state {
            ComputeState::Loading => {}
            ComputeState::Init | ComputeState::Update => {
                if uniforms.enable_compute != 0 || self.state == ComputeState::Init {
                    render_queue.write_buffer(
                        &compute_meta.physics_data,
                        0,
                        bytemuck::cast_slice(&extracted_physics_data.data),
                    );
                    render_queue.write_buffer(
                        &compute_meta.animation_data,
                        0,
                        bytemuck::cast_slice(&extracted_animation_data.data),
                    );
                    render_queue.write_buffer(
                        &trace_meta.storage,
                        0,
                        bytemuck::cast_slice(&vec![0u8; extracted_gh.buffer_size]),
                    );

                    let update_pipeline = pipeline_cache
                        .get_compute_pipeline(pipeline.update_pipeline)
                        .unwrap();
                    let physics_pipeline = pipeline_cache
                        .get_compute_pipeline(pipeline.physics_pipeline)
                        .unwrap();
                    let animation_pipeline = pipeline_cache
                        .get_compute_pipeline(pipeline.animation_pipeline)
                        .unwrap();
                    let rebuild_pipeline = pipeline_cache
                        .get_compute_pipeline(pipeline.rebuild_pipeline)
                        .unwrap();

                    pass.set_pipeline(update_pipeline);
                    pass.dispatch_workgroups(
                        extracted_gh.texture_size,
                        extracted_gh.texture_size,
                        extracted_gh.texture_size,
                    );

                    let dispatch_size =
                        (extracted_physics_data.data[0] as f32).cbrt().ceil() as u32;
                    if dispatch_size > 0 {
                        pass.set_pipeline(physics_pipeline);
                        pass.dispatch_workgroups(dispatch_size, dispatch_size, dispatch_size);
                    }

                    let dispatch_size =
                        (extracted_animation_data.data[0] as f32).cbrt().ceil() as u32;
                    if dispatch_size > 0 {
                        pass.set_pipeline(animation_pipeline);
                        pass.dispatch_workgroups(dispatch_size, dispatch_size, dispatch_size);
                    }

                    pass.set_pipeline(rebuild_pipeline);
                    pass.dispatch_workgroups(
                        extracted_gh.texture_size,
                        extracted_gh.texture_size,
                        extracted_gh.texture_size,
                    );
                }
            }
        }

        Ok(())
    }
}

struct ComputePipeline {
    compute_bind_group_layout: BindGroupLayout,
    update_pipeline: CachedComputePipelineId,
    physics_pipeline: CachedComputePipelineId,
    animation_pipeline: CachedComputePipelineId,
    rebuild_pipeline: CachedComputePipelineId,
}

impl FromWorld for ComputePipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let compute_bind_group_layout =
            render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(std::mem::size_of::<
                                trace::ExtractedUniforms,
                            >()
                                as u64),
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(4),
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::StorageTexture {
                            access: StorageTextureAccess::ReadWrite,
                            format: TextureFormat::R16Uint,
                            view_dimension: TextureViewDimension::D3,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 3,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(4),
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 4,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(4),
                        },
                        count: None,
                    },
                ],
            });

        let compute_shader = world.resource::<AssetServer>().load("compute.wgsl");

        let mut pipeline_cache = world.resource_mut::<PipelineCache>();

        let update_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: None,
            layout: Some(vec![compute_bind_group_layout.clone()]),
            shader: compute_shader.clone(),
            shader_defs: vec![],
            entry_point: Cow::from("update"),
        });
        let physics_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: None,
            layout: Some(vec![compute_bind_group_layout.clone()]),
            shader: compute_shader.clone(),
            shader_defs: vec![],
            entry_point: Cow::from("update_physics"),
        });
        let animation_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: None,
            layout: Some(vec![compute_bind_group_layout.clone()]),
            shader: compute_shader.clone(),
            shader_defs: vec![],
            entry_point: Cow::from("update_animation"),
        });
        let rebuild_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: None,
            layout: Some(vec![compute_bind_group_layout.clone()]),
            shader: compute_shader.clone(),
            shader_defs: vec![],
            entry_point: Cow::from("rebuild_gh"),
        });

        ComputePipeline {
            compute_bind_group_layout,
            update_pipeline,
            physics_pipeline,
            animation_pipeline,
            rebuild_pipeline,
        }
    }
}

struct ComputeImageBindGroup(BindGroup);

fn queue_bind_group(
    mut commands: Commands,
    compute_pipeline: Res<ComputePipeline>,
    render_device: Res<RenderDevice>,
    trace_meta: Res<trace::TraceMeta>,
    compute_meta: Res<ComputeMeta>,
) {
    let bind_group = render_device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &compute_pipeline.compute_bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: trace_meta.uniform.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: trace_meta.storage.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&trace_meta.texture_view),
            },
            BindGroupEntry {
                binding: 3,
                resource: compute_meta.physics_data.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: compute_meta.animation_data.as_entire_binding(),
            },
        ],
    });
    commands.insert_resource(ComputeImageBindGroup(bind_group));
}
