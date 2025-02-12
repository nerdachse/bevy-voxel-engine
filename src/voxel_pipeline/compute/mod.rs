use bevy::{
    asset::load_internal_asset,
    prelude::*,
    reflect::TypeUuid,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        RenderApp, RenderSet,
    },
    utils::HashMap,
};

pub mod animation;
pub mod automata;
pub mod clear;
pub mod mip;
pub mod physics;
pub mod rebuild;

const MAX_TYPE_BUFFER_DATA: usize = 1000000; // 4mb

pub const ANIMATION_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 7356431584756113968);
pub const AUTOMATA_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 2461997473694366307);
pub const CLEAR_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 15320669235097444653);
pub const MIP_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 6189143918759879663);
pub const PHYSICS_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 5103938181551247167);
pub const REBUILD_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 18135969847573717619);

pub struct ComputeResourcesPlugin;

impl Plugin for ComputeResourcesPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            ANIMATION_SHADER_HANDLE,
            "../shaders/compute/animation.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            AUTOMATA_SHADER_HANDLE,
            "../shaders/compute/automata.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            CLEAR_SHADER_HANDLE,
            "../shaders/compute/clear.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            MIP_SHADER_HANDLE,
            "../shaders/compute/mip.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            PHYSICS_SHADER_HANDLE,
            "../shaders/compute/physics.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            REBUILD_SHADER_HANDLE,
            "../shaders/compute/rebuild.wgsl",
            Shader::from_wgsl
        );

        let render_device = app.world.resource::<RenderDevice>();
        let render_queue = app.world.resource::<RenderQueue>();

        let mut uniform_buffer = UniformBuffer::from(ComputeUniforms {
            time: 0.0,
            delta_time: 0.0,
        });
        uniform_buffer.write_buffer(render_device, render_queue);

        let physics_buffer_gpu = render_device.create_buffer_with_data(&BufferInitDescriptor {
            contents: bytemuck::cast_slice(&vec![0u32; MAX_TYPE_BUFFER_DATA]),
            label: None,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        });
        let physics_buffer_cpu = render_device.create_buffer_with_data(&BufferInitDescriptor {
            contents: bytemuck::cast_slice(&vec![0u32; MAX_TYPE_BUFFER_DATA]),
            label: None,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        });
        let animation_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            contents: bytemuck::cast_slice(&vec![0u32; MAX_TYPE_BUFFER_DATA]),
            label: None,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        let bind_group_layout =
            render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("compute bind group layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(ComputeUniforms::SHADER_SIZE.into()),
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
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(4),
                        },
                        count: None,
                    },
                ],
            });

        let bind_group = render_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: physics_buffer_gpu.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: animation_buffer.as_entire_binding(),
                },
            ],
        });

        app.insert_resource(PhysicsData {
            dispatch_size: 0,
            buffer_length: 0,
            entities: HashMap::new(),
            physics_buffer_gpu,
            physics_buffer_cpu,
        })
        .insert_resource(AnimationData {
            dispatch_size: 0,
            animation_buffer,
        })
        .add_plugin(ExtractResourcePlugin::<PhysicsData>::default())
        .add_plugin(ExtractResourcePlugin::<AnimationData>::default());

        app.sub_app_mut(RenderApp)
            .insert_resource(ComputeData {
                bind_group_layout,
                bind_group,
                uniform_buffer,
            })
            .init_resource::<clear::Pipeline>()
            .init_resource::<rebuild::Pipeline>()
            .init_resource::<automata::Pipeline>()
            .init_resource::<physics::Pipeline>()
            .init_resource::<animation::Pipeline>()
            .init_resource::<mip::Pipeline>()
            .add_system(prepare_uniforms.in_set(RenderSet::Prepare));
    }
}

fn prepare_uniforms(
    time: Res<Time>,
    mut compute_data: ResMut<ComputeData>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    let uniforms = ComputeUniforms {
        time: time.elapsed_seconds_f64() as f32,
        delta_time: time.delta_seconds() as f32,
    };
    compute_data.uniform_buffer.set(uniforms);
    compute_data
        .uniform_buffer
        .write_buffer(&render_device, &render_queue);
}

#[derive(Resource, ShaderType)]
struct ComputeUniforms {
    time: f32,
    delta_time: f32,
}

#[derive(Clone, Resource, ExtractResource)]
pub struct PhysicsData {
    pub dispatch_size: u32,
    pub buffer_length: u64,
    pub entities: HashMap<Entity, usize>,
    pub physics_buffer_gpu: Buffer,
    pub physics_buffer_cpu: Buffer,
}

#[derive(Clone, Resource, ExtractResource)]
pub struct AnimationData {
    pub dispatch_size: u32,
    pub animation_buffer: Buffer,
}

#[derive(Resource)]
pub struct ComputeData {
    pub bind_group_layout: BindGroupLayout,
    pub bind_group: BindGroup,
    uniform_buffer: UniformBuffer<ComputeUniforms>,
}
