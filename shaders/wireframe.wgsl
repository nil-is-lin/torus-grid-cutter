// 3D cylinder line rendering with smooth shading.

struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) v_color: vec4<f32>,
    @location(1) v_normal_ws: vec3<f32>,
}

struct LightData {
    direction: vec4<f32>,
    color:     vec4<f32>,
};

struct SceneUniform {
    view_proj:        mat4x4<f32>,
    camera_position:  vec4<f32>,
    light0:           LightData,
    light1:           LightData,
    light2:           LightData,
    ambient_color:    vec4<f32>,
    bg_color:         vec4<f32>,
    shader_params:    vec4<f32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.v_color = in.color;
    out.v_normal_ws = normalize(in.normal);
    out.clip_position = scene.view_proj * vec4<f32>(in.pos, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple directional light for smooth shading
    let light_dir = normalize(vec3<f32>(0.3, 0.8, 0.5));
    let ambient = 0.4;
    let n_dot_l = max(dot(normalize(in.v_normal_ws), light_dir), 0.0);
    let brightness = ambient + (1.0 - ambient) * n_dot_l;

    return vec4<f32>(in.v_color.rgb * brightness, in.v_color.a);
}
