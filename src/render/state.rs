use super::pipeline::SceneUniform;
use super::wireframe;
use crate::ui::panel::UiState;
use wgpu::util::DeviceExt;

/// 3D 场景抗锯齿采样数（depth 纹理与 MSAA 颜色纹理共用）
pub const MSAA_SAMPLES: u32 = 4;

/// 创建 4x MSAA 颜色纹理视图（3D 场景先渲染到此，再 resolve 到窗口 surface）
fn create_msaa_view(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("MSAA Color Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: MSAA_SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: surface_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub struct ColoredEdgeData {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub num_vertices: u32,
    pub uniform_bind_group: wgpu::BindGroup,
}

pub struct RenderState {
    pub mesh_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    surface_format: wgpu::TextureFormat,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    /// 4x MSAA 颜色纹理：3D 场景先渲染到这里，最后 resolve 到窗口 surface
    pub msaa_view: wgpu::TextureView,
    pub mesh_edges: Option<ColoredEdgeData>,
    pub patch_edges: Option<ColoredEdgeData>,
}

impl RenderState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader: &wgpu::ShaderModule,
        wireframe_shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        vertices: &[crate::mesh::vertex::GpuVertex],
        indices: &[u32],
        mesh: Option<&crate::mesh::half_edge::HalfEdgeMesh>,
        ui_state: &UiState,
        major_r: f64,
        minor_r: f64,
    ) -> Self {
        let mesh_pipeline = super::pipeline::create_mesh_pipeline(device, shader, surface_format);

        let (vertex_buffer, index_buffer, num_indices) =
            super::mesh_buffer::upload_mesh(device, vertices, indices);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scene Uniform"),
            contents: bytemuck::cast_slice(&[SceneUniform::new()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = mesh_pipeline.get_bind_group_layout(0);

        // Create default 1x1 white texture (used when no texture is loaded)
        let default_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Default White Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &default_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let default_tex_view = default_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let default_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Default Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&default_tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&default_sampler),
                },
            ],
        });

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // 4x MSAA 颜色纹理（3D 场景渲染目标，最后 resolve 到窗口）
        let msaa_view = create_msaa_view(device, surface_format, width, height);

        let (mesh_edges, patch_edges) = if let Some(mesh) = mesh {
            build_edge_data(
                device,
                wireframe_shader,
                surface_format,
                &uniform_buffer,
                mesh,
                ui_state,
                major_r,
                minor_r,
            )
        } else {
            (None, None)
        };

        RenderState {
            mesh_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            uniform_buffer,
            uniform_bind_group,
            surface_format,
            depth_texture,
            depth_view,
            msaa_view,
            mesh_edges,
            patch_edges,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_mesh_buffers(
        &mut self,
        device: &wgpu::Device,
        wireframe_shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
        vertices: &[crate::mesh::vertex::GpuVertex],
        indices: &[u32],
        mesh: Option<&crate::mesh::half_edge::HalfEdgeMesh>,
        ui_state: &UiState,
        major_r: f64,
        minor_r: f64,
    ) {
        let (vertex_buffer, index_buffer, num_indices) =
            super::mesh_buffer::upload_mesh(device, vertices, indices);
        self.vertex_buffer = vertex_buffer;
        self.index_buffer = index_buffer;
        self.num_indices = num_indices;

        let (mesh_edges, patch_edges) = if let Some(mesh) = mesh {
            build_edge_data(
                device,
                wireframe_shader,
                surface_format,
                &self.uniform_buffer,
                mesh,
                ui_state,
                major_r,
                minor_r,
            )
        } else {
            (None, None)
        };
        self.mesh_edges = mesh_edges;
        self.patch_edges = patch_edges;
    }

    pub fn update_depth_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.msaa_view = create_msaa_view(device, self.surface_format, width, height);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_edge_data(
    device: &wgpu::Device,
    wireframe_shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
    uniform_buffer: &wgpu::Buffer,
    mesh: &crate::mesh::half_edge::HalfEdgeMesh,
    ui_state: &UiState,
    major_r: f64,
    minor_r: f64,
) -> (Option<ColoredEdgeData>, Option<ColoredEdgeData>) {
    let skip_seams = ui_state.view_mode == crate::ui::panel::ViewMode::Unfolded;
    let mesh_edges = wireframe::classify_mesh_edges(mesh, skip_seams, major_r, minor_r);

    let mesh_edge_color: [f32; 4] = [0.3, 0.3, 0.3, 0.6];
    let mesh_edge_width = ui_state.mesh_edge_width * wireframe::line_width_scale(ui_state);

    let me_data = if !mesh_edges.is_empty() && mesh_edge_width > 0.0 {
        let me_segments: Vec<wireframe::ColoredSegment> = mesh_edges
            .iter()
            .map(|seg| wireframe::ColoredSegment {
                positions: *seg,
                color: mesh_edge_color,
                width: mesh_edge_width,
            })
            .collect();
        let (me_pipeline, me_bg_layout) = wireframe::create_patch_edge_pipeline(
            device,
            wireframe_shader,
            surface_format,
            -10, // 负 bias：网格边线浮于表面之上
        );
        let me_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mesh Edge Camera Bind Group"),
            layout: &me_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let vertex_data = wireframe::colored_segments_to_vertex_data(&me_segments);
        let max_bytes = device.limits().max_buffer_size;
        let num_vertices = vertex_data.len() as u32;
        let bytes = num_vertices as u64 * std::mem::size_of::<[f32; 10]>() as u64;
        if bytes > max_bytes {
            // 网格过大（如大量切割线后 8 万+ 面）：边线缓冲超 GPU 上限——
            // 降级跳过边线渲染（面本身照常），避免 create_buffer 崩溃。
            log::warn!(
                "Mesh edge buffer too large ({} B > max {} B), skipping mesh edge rendering",
                bytes,
                max_bytes
            );
            None
        } else {
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mesh Edge Buffer"),
                contents: bytemuck::cast_slice(&vertex_data),
                usage: wgpu::BufferUsages::VERTEX,
            });
            Some(ColoredEdgeData {
                pipeline: me_pipeline,
                vertex_buffer,
                num_vertices,
                uniform_bind_group: me_bind_group,
            })
        }
    } else {
        None
    };

    // Use the cut mesh (with on_cut markers) instead of base mesh for loop segments,
    // so the rendered cut lines exactly coincide with patch coloring boundaries.
    let loop_segments = wireframe::generate_loop_segments(mesh, ui_state);

    let pe_data = if !loop_segments.is_empty() {
        let (pe_pipeline, pe_bg_layout) = wireframe::create_patch_edge_pipeline(
            device,
            wireframe_shader,
            surface_format,
            -20, // 更大负 bias：切割线浮于网格边线之上
        );
        let pe_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Patch Edge Camera Bind Group"),
            layout: &pe_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let vertex_data = wireframe::colored_segments_to_vertex_data(&loop_segments);
        let max_bytes = device.limits().max_buffer_size;
        let num_vertices = vertex_data.len() as u32;
        let bytes = num_vertices as u64 * std::mem::size_of::<[f32; 10]>() as u64;
        if bytes > max_bytes {
            log::warn!(
                "Patch edge buffer too large ({} B > max {} B), skipping cut line rendering",
                bytes,
                max_bytes
            );
            None
        } else {
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Patch Edge Buffer"),
                contents: bytemuck::cast_slice(&vertex_data),
                usage: wgpu::BufferUsages::VERTEX,
            });
            Some(ColoredEdgeData {
                pipeline: pe_pipeline,
                vertex_buffer,
                num_vertices,
                uniform_bind_group: pe_bind_group,
            })
        }
    } else {
        None
    };

    (me_data, pe_data)
}
