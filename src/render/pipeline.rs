use crate::mesh::vertex::GpuVertex;

/// Extended scene uniform — matches SceneUniform in mesh.wgsl.
/// Layout:
///   view_proj        mat4x4<f32>  offset 0   size 64
///   camera_position  vec4<f32>    offset 64  size 16  (xyz=pos, w=shader_mode)
///   light0           vec4x2       offset 80  size 32
///   light1           vec4x2       offset 112 size 32
///   light2           vec4x2       offset 144 size 32
///   ambient_color    vec4<f32>    offset 176 size 16
///   bg_color         vec4<f32>    offset 192 size 16
///   shader_params    vec4<f32>    offset 208 size 16
///   Total: 224 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SceneUniform {
    pub view_proj: [[f32; 4]; 4],  // 64
    pub camera_position: [f32; 4], // 16  xyz=pos, w=shader_mode as f32
    pub light0_dir: [f32; 4],      // 16  xyz=dir, w=intensity
    pub light0_color: [f32; 4],    // 16
    pub light1_dir: [f32; 4],      // 16
    pub light1_color: [f32; 4],    // 16
    pub light2_dir: [f32; 4],      // 16
    pub light2_color: [f32; 4],    // 16
    pub ambient_color: [f32; 4],   // 16  rgb=color, a=intensity
    pub bg_color: [f32; 4],        // 16  rgb=background, a=ao_strength
    pub shader_params: [f32; 4],   // 16  x=roughness, y=metallic, z=specular, w=shininess
}

impl SceneUniform {
    pub fn new() -> Self {
        SceneUniform {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            camera_position: [0.0, 0.0, 8.0, 0.0],
            light0_dir: [-0.577, -0.577, -0.577, 1.0],
            light0_color: [1.0, 0.98, 0.95, 0.0],
            light1_dir: [0.577, -0.289, 0.577, 0.6],
            light1_color: [0.85, 0.9, 1.0, 0.0],
            light2_dir: [0.0, 0.577, -0.577, 0.4],
            light2_color: [1.0, 0.95, 0.9, 0.0],
            ambient_color: [0.95, 0.95, 1.0, 0.25],
            bg_color: [1.0, 1.0, 1.0, 0.5],
            shader_params: [0.5, 0.0, 0.5, 32.0],
        }
    }
}

/// Create the mesh render pipeline with extended scene uniform.
pub fn create_mesh_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scene Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Mesh Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let depth_format = wgpu::TextureFormat::Depth32Float;

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Mesh Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[GpuVertex::layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: crate::render::state::MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}
