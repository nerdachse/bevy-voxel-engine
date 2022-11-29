use bevy::{
    core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    prelude::*,
    render::{
        render_resource::{*},
        renderer::{RenderDevice, RenderQueue},
        view::ViewTarget,
        RenderApp,
    },
};
pub use node::DenoiseNode;

mod node;

pub struct DenoisePlugin;

impl Plugin for DenoisePlugin {
    fn build(&self, app: &mut App) {
        app.sub_app_mut(RenderApp)
            .init_resource::<DenoisePipeline>();
    }
}

#[derive(Resource)]
struct DenoisePipeline {
    bind_group_layout: BindGroupLayout,
    pass_data_bind_group_layout: BindGroupLayout,
    pipeline_id: CachedRenderPipelineId,
    uniform_buffer: Buffer,
    pass_data: DynamicUniformBuffer<PassData>,
}

#[derive(Component)]
struct ViewDenoisePipeline(CachedRenderPipelineId);

impl FromWorld for DenoisePipeline {
    fn from_world(render_world: &mut World) -> Self {
        let bind_group_layout = render_world
            .resource::<RenderDevice>()
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("denoise bind group layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(
                                get_uniform_buffer_data().len() as u64
                            ),
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::StorageTexture {
                            access: StorageTextureAccess::ReadWrite,
                            format: TextureFormat::Rgba8Unorm,
                            view_dimension: TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 3,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::StorageTexture {
                            access: StorageTextureAccess::ReadWrite,
                            format: TextureFormat::Rgba16Float,
                            view_dimension: TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });
        let pass_data_bind_group_layout = render_world
            .resource::<RenderDevice>()
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("denoise bind group layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: BufferSize::new(
                                std::mem::size_of::<PassData>() as u64
                            ),
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            sample_type: TextureSampleType::Float { filterable: false },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let asset_server = render_world.get_resource::<AssetServer>().unwrap();
        let shader = asset_server.load("denoise.wgsl");

        let pipeline_descriptor = RenderPipelineDescriptor {
            label: Some("denoise pipeline".into()),
            layout: Some(vec![
                bind_group_layout.clone(),
                pass_data_bind_group_layout.clone(),
            ]),
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader,
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: ViewTarget::TEXTURE_FORMAT_HDR,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
        };

        let mut cache = render_world.resource_mut::<PipelineCache>();
        let pipeline_id = cache.queue_render_pipeline(pipeline_descriptor);

        let uniform_buffer = render_world
            .resource::<RenderDevice>()
            .create_buffer_with_data(&BufferInitDescriptor {
                label: Some("denoise uniform buffer"),
                contents: &get_uniform_buffer_data(),
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            });

        let mut pass_data = DynamicUniformBuffer::default();
        pass_data.push(PassData::new(1.0));
        pass_data.push(PassData::new(2.0));
        pass_data.push(PassData::new(4.0));
        pass_data.write_buffer(
            render_world.resource::<RenderDevice>(),
            render_world.resource::<RenderQueue>(),
        );

        DenoisePipeline {
            bind_group_layout,
            pass_data_bind_group_layout,
            pipeline_id,
            uniform_buffer,
            pass_data,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, ShaderType)]
struct PassData {
    denoise_strength: f32,
    padding1: u32,
    padding2: u32,
    padding3: u32,
    padding4: [UVec4; 15],
}
impl PassData {
    fn new(denoise_strength: f32) -> Self {
        Self {
            denoise_strength,
            padding1: 0,
            padding2: 0,
            padding3: 0,
            padding4: [UVec4::ZERO; 15],
        }
    }
}

fn get_uniform_buffer_data() -> Vec<u8> {
    #[cfg_attr(rustfmt, rustfmt_skip)]
    let offsets: [(f32, f32); 25] = [
        (-2.0, -2.0), (-1.0, -2.0), (0.0, -2.0), (1.0, -2.0), (2.0, -2.0),
        (-2.0, -1.0), (-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (2.0, -1.0),
        (-2.0, 0.0),  (-1.0, 0.0),  (0.0, 0.0),  (1.0, 0.0),  (2.0, 0.0),
        (-2.0, 1.0),  (-1.0, 1.0),  (0.0, 1.0),  (1.0, 1.0),  (2.0, 1.0),
        (-2.0, 2.0),  (-1.0, 2.0),  (0.0, 2.0),  (1.0, 2.0),  (2.0, 2.0),
    ];

    #[cfg_attr(rustfmt, rustfmt_skip)]
    let kernel: [f32; 25] = [
        1.0/256.0, 1.0/64.0, 3.0/128.0, 1.0/64.0, 1.0/256.0,
        1.0/64.0,  1.0/16.0, 3.0/32.0,  1.0/16.0, 1.0/64.0,
        3.0/128.0, 3.0/32.0, 9.0/64.0,  3.0/32.0, 3.0/128.0,
        1.0/64.0,  1.0/16.0, 3.0/32.0,  1.0/16.0, 1.0/64.0,
        1.0/256.0, 1.0/64.0, 3.0/128.0, 1.0/64.0, 1.0/256.0,
    ];

    let mut data = Vec::new();
    for i in 0..25 {
        data.extend_from_slice(&offsets[i].0.to_le_bytes());
        data.extend_from_slice(&offsets[i].1.to_le_bytes());
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0i32.to_le_bytes());
    }
    for i in 0..25 {
        data.extend_from_slice(&kernel[i].to_le_bytes());
        data.extend_from_slice(&1f32.to_le_bytes());
        data.extend_from_slice(&1f32.to_le_bytes());
        data.extend_from_slice(&1f32.to_le_bytes());
    }

    data
}