use crate::mesh::vertex::GpuVertex;
use wgpu::util::DeviceExt;

pub fn upload_mesh(
    device: &wgpu::Device,
    vertices: &[GpuVertex],
    indices: &[u32],
) -> (wgpu::Buffer, wgpu::Buffer, u32) {
    if vertices.is_empty() || indices.is_empty() {
        let dummy_vert = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dummy Vertex Buffer"),
            contents: bytemuck::cast_slice(&[GpuVertex::default()]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let dummy_idx = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Dummy Index Buffer"),
            contents: bytemuck::cast_slice(&[0u32]),
            usage: wgpu::BufferUsages::INDEX,
        });
        return (dummy_vert, dummy_idx, 0);
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    (vertex_buffer, index_buffer, indices.len() as u32)
}
