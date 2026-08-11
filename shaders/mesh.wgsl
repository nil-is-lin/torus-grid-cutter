// ============================================================
// Professional Mesh Shader — 42 Rendering Modes
// Enhanced X-Ray, Glass + 12 new visual shaders
// ============================================================

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) color:    vec4<f32>,
    @location(3) uv:       vec2<f32>,
    @location(4) material: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)  v_color:      vec4<f32>,
    @location(1)  v_normal:     vec3<f32>,
    @location(2)  v_world_pos:  vec3<f32>,
    @location(3)  v_uv:         vec2<f32>,
    @location(4)  v_material:   f32,
};

struct LightData {
    direction: vec4<f32>,   // xyz = direction, w = intensity
    color:     vec4<f32>,   // rgb = color, a = unused
};

struct SceneUniform {
    view_proj:        mat4x4<f32>,
    camera_position:  vec4<f32>,   // xyz = position, w = shader_mode
    light0:           LightData,
    light1:           LightData,
    light2:           LightData,
    ambient_color:    vec4<f32>,   // rgb = color, a = ambient_intensity
    bg_color:         vec4<f32>,   // rgb = background, a = ao_strength
    shader_params:    vec4<f32>,   // x = roughness, y = metallic, z = specular, w = shininess
};

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

@group(0) @binding(1)
var scene_texture: texture_2d<f32>;
@group(0) @binding(2)
var scene_sampler: sampler;

// ---- Constants ----
const PI: f32 = 3.14159265359;
const TWO_PI: f32 = 6.28318530718;

// ---- Vertex Shader ----
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(in.position, 1.0);
    out.v_color      = in.color;
    out.v_normal     = in.normal;
    out.v_world_pos  = in.position;
    out.v_uv         = in.uv;
    out.v_material   = in.material;
    return out;
}

// ============================================================
// Helper functions
// ============================================================

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len = length(v);
    if (len < 1e-8) { return vec3<f32>(0.0, 1.0, 0.0); }
    return v / len;
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom + 1e-8);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    let g1_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let g1_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return g1_v * g1_l;
}

fn compute_lighting(
    n: vec3<f32>, l: vec3<f32>, v: vec3<f32>,
    base_color: vec3<f32>, light_color: vec3<f32>,
    intensity: f32, roughness: f32, metallic: f32,
) -> vec3<f32> {
    let h = safe_normalize(l + v);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.001);
    let n_dot_h = max(dot(n, h), 0.0);
    let h_dot_v = max(dot(h, v), 0.0);

    let f0 = mix(vec3<f32>(0.04), base_color, metallic);
    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    let f = fresnel_schlick(h_dot_v, f0);

    let specular = (d * g * f) / (4.0 * n_dot_v * n_dot_l + 0.001);
    let kD = (vec3<f32>(1.0) - f) * (1.0 - metallic);
    let diffuse = kD * base_color / PI;

    return (diffuse + specular) * light_color * intensity * n_dot_l;
}

fn three_point_pbr(
    n: vec3<f32>, v: vec3<f32>, base_color: vec3<f32>,
    roughness: f32, metallic: f32,
) -> vec3<f32> {
    let ambient = scene.ambient_color.rgb * scene.ambient_color.a * base_color;
    var result = ambient;
    result += compute_lighting(n, safe_normalize(-scene.light0.direction.xyz), v,
        base_color, scene.light0.color.rgb, scene.light0.direction.w, roughness, metallic);
    result += compute_lighting(n, safe_normalize(-scene.light1.direction.xyz), v,
        base_color, scene.light1.color.rgb, scene.light1.direction.w, roughness, metallic);
    result += compute_lighting(n, safe_normalize(-scene.light2.direction.xyz), v,
        base_color, scene.light2.color.rgb, scene.light2.direction.w, roughness, metallic);
    return result;
}

fn simple_three_point(n: vec3<f32>, base_color: vec3<f32>, spec_strength: f32, shininess: f32) -> vec3<f32> {
    let v = safe_normalize(scene.camera_position.xyz - n * 0.001);
    let ambient = scene.ambient_color.rgb * scene.ambient_color.a;
    var total = ambient * base_color;
    // Light 0
    let l0 = safe_normalize(-scene.light0.direction.xyz);
    let h0 = safe_normalize(l0 + v);
    total += base_color * max(dot(n, l0), 0.0) * scene.light0.color.rgb * scene.light0.direction.w;
    total += scene.light0.color.rgb * pow(max(dot(n, h0), 0.0), shininess) * spec_strength * scene.light0.direction.w;
    // Light 1
    let l1 = safe_normalize(-scene.light1.direction.xyz);
    let h1 = safe_normalize(l1 + v);
    total += base_color * max(dot(n, l1), 0.0) * scene.light1.color.rgb * scene.light1.direction.w * 0.6;
    total += scene.light1.color.rgb * pow(max(dot(n, h1), 0.0), shininess) * spec_strength * 0.6 * scene.light1.direction.w;
    // Light 2
    let l2 = safe_normalize(-scene.light2.direction.xyz);
    let h2 = safe_normalize(l2 + v);
    total += base_color * max(dot(n, l2), 0.0) * scene.light2.color.rgb * scene.light2.direction.w * 0.4;
    total += scene.light2.color.rgb * pow(max(dot(n, h2), 0.0), shininess) * spec_strength * 0.4 * scene.light2.direction.w;
    return total;
}

fn rgb_to_hsv(rgb: vec3<f32>) -> vec3<f32> {
    let v = max(max(rgb.r, rgb.g), rgb.b);
    let c = v - min(min(rgb.r, rgb.g), rgb.b);
    let s = select(0.0, c / (v + 1e-8), v > 0.0);
    var h: f32 = 0.0;
    if (c > 1e-8) {
        if (v == rgb.r) { h = (rgb.g - rgb.b) / c; }
        else if (v == rgb.g) { h = 2.0 + (rgb.b - rgb.r) / c; }
        else { h = 4.0 + (rgb.r - rgb.g) / c; }
    }
    h = h / 6.0;
    if (h < 0.0) { h += 1.0; }
    return vec3<f32>(h, s, v);
}

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let h = hsv.x * 6.0;
    let s = hsv.y;
    let v = hsv.z;
    let c = v * s;
    let x = c * (1.0 - abs(h % 2.0 - 1.0));
    let m = v - c;
    var rgb: vec3<f32>;
    let hi = i32(h) % 6;
    if (hi == 0) { rgb = vec3<f32>(c, x, 0.0); }
    else if (hi == 1) { rgb = vec3<f32>(x, c, 0.0); }
    else if (hi == 2) { rgb = vec3<f32>(0.0, c, x); }
    else if (hi == 3) { rgb = vec3<f32>(0.0, x, c); }
    else if (hi == 4) { rgb = vec3<f32>(x, 0.0, c); }
    else { rgb = vec3<f32>(c, 0.0, x); }
    return rgb + vec3<f32>(m);
}

// ============================================================
// Fragment Shader — Dispatches to 42 rendering modes
// Per-patch: v_material < -0.5 encodes shader mode as -1-mode
// Global:    uniform camera_position.w (when < 100)
// ============================================================
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = safe_normalize(in.v_normal);
    let v = safe_normalize(scene.camera_position.xyz - in.v_world_pos);
    let roughness = scene.shader_params.x;
    let metallic  = scene.shader_params.y;
    let spec_int  = scene.shader_params.z;
    let shininess = scene.shader_params.w;
    let pos = in.v_world_pos;
    let ndv = max(dot(n, v), 0.0);

    // Per-patch shader dispatch:
    //   global_mode >= 100 means per-patch mode is active (sentinel from CPU)
    //   v_material < -0.5 encodes the per-patch shader mode index
    let global_mode_raw = i32(scene.camera_position.w);
    var mode: i32;
    var base: vec3<f32>;
    var alpha: f32;
    if (global_mode_raw >= 100) {
        // Per-patch mode active — decode from material
        if (in.v_material < -0.5) {
            mode = i32(-1.0 - in.v_material + 0.5);
        } else {
            mode = 0; // fallback to PBR for faces without per-patch override
        }
        base = in.v_color.rgb;
        alpha = in.v_color.a;
    } else {
        mode = global_mode_raw;
        base = in.v_color.rgb;
        alpha = in.v_color.a;
    }

    // Texture sampling — default 1x1 white texture is a no-op multiply
    // When a real texture is loaded, PBR modes will use it automatically
    let tex_color = textureSample(scene_texture, scene_sampler, in.v_uv);
    if (mode == 0 || mode == 1 || mode == 2 || mode == 26 || mode == 27) {
        base = base * tex_color.rgb;
    }

    var result: vec3<f32>;
    var out_alpha: f32 = alpha;

    switch mode {
        // ======== 0: PBR Metallic-Roughness ========
        case 0: {
            result = three_point_pbr(n, v, base, roughness, metallic);
        }
        // ======== 1: Enhanced Blinn-Phong ========
        case 1: {
            result = simple_three_point(n, base, spec_int, shininess);
        }
        // ======== 2: Clay / Studio ========
        case 2: {
            let clay_color = vec3<f32>(0.85, 0.78, 0.70);
            result = simple_three_point(n, clay_color, 0.15, 16.0);
        }
        // ======== 3: Matcap-style ========
        case 3: {
            let rim = pow(1.0 - ndv, 2.0);
            let lit = simple_three_point(n, base, 0.3, 32.0);
            result = lit + vec3<f32>(rim * 0.15);
        }
        // ======== 4: Normal Visualization ========
        case 4: {
            result = n * 0.5 + vec3<f32>(0.5);
        }
        // ======== 5: Toon Shading ========
        case 5: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let n_dot_l = dot(n, l0);
            var shade: f32;
            if (n_dot_l > 0.6) { shade = 1.0; }
            else if (n_dot_l > 0.3) { shade = 0.7; }
            else if (n_dot_l > 0.0) { shade = 0.45; }
            else { shade = 0.25; }
            let rim = pow(1.0 - ndv, 3.0);
            result = base * shade + vec3<f32>(rim * 0.2);
        }
        // ======== 6: Rim Lighting ========
        case 6: {
            let lit = simple_three_point(n, base, 0.2, 16.0);
            let rim = pow(1.0 - ndv, 3.0);
            let rim_color = vec3<f32>(0.4, 0.7, 1.0);
            result = lit * 0.6 + rim_color * rim * 1.5;
        }
        // ======== 7: Wireframe Overlay (solid color) ========
        case 7: {
            result = vec3<f32>(0.15, 0.15, 0.15);
            out_alpha = 0.9;
        }

        // ======== 8: X-Ray ENHANCED — multi-layer glow + scan + depth ========
        case 8: {
            let edge = 1.0 - ndv;

            // Layer 1: Soft outer glow (wide, blue)
            let glow1 = pow(edge, 1.5) * vec3<f32>(0.1, 0.3, 0.8) * 2.0;
            // Layer 2: Sharp edge highlight (narrow, cyan)
            let glow2 = pow(edge, 4.0) * vec3<f32>(0.3, 0.8, 1.0) * 3.5;
            // Layer 3: Hot-white edge core
            let glow3 = pow(edge, 8.0) * vec3<f32>(1.0, 1.0, 1.0) * 2.0;

            // Scan lines across the surface
            let scan_freq = 30.0;
            let scan1 = pow(sin(pos.y * scan_freq + pos.x * 12.0) * 0.5 + 0.5, 3.0);
            let scan2 = pow(sin(pos.z * scan_freq * 0.7 - pos.y * 8.0) * 0.5 + 0.5, 4.0);
            let scan_color = vec3<f32>(0.0, 0.6, 1.0) * (scan1 * 0.25 + scan2 * 0.15);

            // Depth-based color gradient (close=warm, far=cool)
            let depth = length(pos) * 0.3;
            let depth_color = mix(vec3<f32>(0.0, 0.7, 1.0), vec3<f32>(0.5, 0.1, 1.0), clamp(depth, 0.0, 1.0));

            // Back-face differentiation (orange tint for interior structure)
            let facing = dot(n, v);
            let back_tint = select(vec3<f32>(1.0, 0.5, 0.15), vec3<f32>(0.8, 1.0, 1.0), facing >= 0.0);

            // Combine all layers
            result = (glow1 + glow2 + glow3) * back_tint + scan_color * depth_color;

            // Alpha: strong edges visible, center translucent for see-through
            out_alpha = clamp(pow(edge, 1.0) * 0.85 + 0.12, 0.0, 1.0);
        }

        // ======== 9: Fresnel Glow ========
        case 9: {
            let fresnel = pow(1.0 - ndv, 4.0);
            let lit = simple_three_point(n, base * 0.3, 0.1, 8.0);
            let glow_color = mix(vec3<f32>(0.1, 0.3, 0.8), vec3<f32>(0.8, 0.3, 1.0), fresnel);
            result = lit + glow_color * fresnel * 2.0;
        }
        // ======== 10: Iridescence ========
        case 10: {
            let angle = dot(n, v);
            let iri_color = vec3<f32>(
                0.5 + 0.5 * cos(TWO_PI * (angle * 2.0 + 0.0)),
                0.5 + 0.5 * cos(TWO_PI * (angle * 2.0 + 0.33)),
                0.5 + 0.5 * cos(TWO_PI * (angle * 2.0 + 0.67))
            );
            let lit = simple_three_point(n, iri_color, 0.5, 64.0);
            result = lit;
        }
        // ======== 11: Hatching ========
        case 11: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let n_dot_l = max(dot(n, l0), 0.0);
            let world_uv = pos.x * 8.0 + pos.y * 8.0;
            let hatch1 = select(0.3, 1.0, n_dot_l > 0.3);
            let hatch2 = select(0.5, 1.0, n_dot_l > 0.5);
            let line = select(hatch1, hatch2, fract(sin(world_uv * 12.9898) * 43758.5453) > 0.5);
            let shade = mix(0.2, line, smoothstep(0.0, 0.1, n_dot_l));
            result = base * shade;
        }
        // ======== 12: Stippling ========
        case 12: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let n_dot_l = max(dot(n, l0), 0.0);
            let noise = fract(sin(dot(pos.xy * 20.0, vec2<f32>(12.9898, 78.233))) * 43758.5453);
            let threshold = 1.0 - n_dot_l;
            let dot_val = select(0.15, 1.0, noise > threshold);
            result = base * dot_val;
        }
        // ======== 13: Checkerboard UV ========
        case 13: {
            let uv_scaled = in.v_uv * 10.0;
            let checker = select(0.0, 1.0, (i32(floor(uv_scaled.x)) + i32(floor(uv_scaled.y))) % 2 == 0);
            let checker_color = mix(vec3<f32>(0.9, 0.9, 0.9), vec3<f32>(0.2, 0.2, 0.2), checker);
            result = simple_three_point(n, checker_color, 0.2, 16.0);
        }
        // ======== 14: Curvature Heatmap ========
        case 14: {
            let dp = fwidth(n);
            let curvature = length(dp) * 20.0;
            let heat = vec3<f32>(
                min(curvature * 2.0, 1.0),
                max(1.0 - abs(curvature - 0.5) * 2.0, 0.0),
                max(1.0 - curvature * 2.0, 0.0)
            );
            result = simple_three_point(n, heat, 0.1, 8.0);
        }
        // ======== 15: AO Approximation ========
        case 15: {
            let ao = 0.5 + 0.5 * ndv;
            let ao_strength = scene.bg_color.a;
            let ao_factor = mix(1.0, ao, ao_strength);
            result = simple_three_point(n, base, 0.1, 8.0) * ao_factor;
        }
        // ======== 16: Subsurface Scattering ========
        case 16: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let sss_color = vec3<f32>(1.0, 0.4, 0.3);
            let thickness = 0.5;
            let transmittance = exp(-thickness * max(dot(-n, l0), 0.0));
            let diffuse = max(dot(n, l0), 0.0);
            let sss = sss_color * transmittance * 0.5;
            result = base * diffuse * 0.6 + sss + scene.ambient_color.rgb * scene.ambient_color.a * base * 0.3;
        }
        // ======== 17: Clear Coat ========
        case 17: {
            let base_lit = three_point_pbr(n, v, base, 0.6, 0.0);
            let coat_roughness = 0.05;
            let coat_f0 = vec3<f32>(0.04);
            let h = safe_normalize(safe_normalize(-scene.light0.direction.xyz) + v);
            let n_dot_h = max(dot(n, h), 0.0);
            let d = distribution_ggx(n_dot_h, coat_roughness);
            let coat_spec = coat_f0 * d * 0.25;
            result = base_lit * 0.85 + coat_spec;
        }
        // ======== 18: Anisotropic ========
        case 18: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let tangent = safe_normalize(cross(n, vec3<f32>(0.0, 1.0, 0.0)));
            let t_dot_l = dot(tangent, l0);
            let t_dot_v = dot(tangent, v);
            let aniso = sqrt(max(1.0 - t_dot_l * t_dot_l, 0.0)) *
                        sqrt(max(1.0 - t_dot_v * t_dot_v, 0.0)) - t_dot_l * t_dot_v;
            let spec = pow(max(aniso, 0.0), 16.0) * spec_int;
            let diff = max(dot(n, l0), 0.0);
            result = base * (diff * 0.6 + scene.ambient_color.a * 0.3) + vec3<f32>(spec) * scene.light0.color.rgb;
        }
        // ======== 19: Cel-Shading (4 tones) ========
        case 19: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let n_dot_l = dot(n, l0);
            var shade: f32 = 0.2;
            if (n_dot_l > 0.7) { shade = 1.0; }
            else if (n_dot_l > 0.4) { shade = 0.75; }
            else if (n_dot_l > 0.1) { shade = 0.5; }
            result = base * shade;
        }
        // ======== 20: Gooch (warm-cool) ========
        case 20: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let n_dot_l = dot(n, l0) * 0.5 + 0.5;
            let warm = vec3<f32>(0.9, 0.7, 0.4);
            let cool = vec3<f32>(0.2, 0.3, 0.6);
            let gooch = mix(cool, warm, n_dot_l);
            let spec = pow(max(dot(safe_normalize(l0 + v), n), 0.0), 32.0) * 0.4;
            result = base * 0.5 * gooch + vec3<f32>(spec);
        }
        // ======== 21: Hemispheric Lighting ========
        case 21: {
            let sky = vec3<f32>(0.6, 0.7, 0.9);
            let ground = vec3<f32>(0.4, 0.35, 0.25);
            let hemi = mix(ground, sky, n.y * 0.5 + 0.5);
            result = base * hemi;
        }
        // ======== 22: Contour / Silhouette ========
        case 22: {
            let edge = 1.0 - ndv;
            let contour = smoothstep(0.6, 0.9, edge);
            let lit = simple_three_point(n, base, 0.1, 8.0);
            result = mix(lit, vec3<f32>(0.05, 0.05, 0.1), contour);
        }
        // ======== 23: Flat Shading ========
        case 23: {
            let flat_n = safe_normalize(cross(dpdxFine(pos), dpdyFine(pos)));
            result = simple_three_point(flat_n, base, 0.2, 16.0);
        }

        // ======== 24: Glass ENHANCED — chromatic aberration + env reflection + depth ========
        case 24: {
            let fresnel = pow(1.0 - ndv, 3.5);

            // Per-channel chromatic aberration (simulates light dispersion)
            let refract_r = pow(1.0 - ndv, 2.0);
            let refract_g = pow(1.0 - ndv, 2.5);
            let refract_b = pow(1.0 - ndv, 3.0);
            let chromatic = vec3<f32>(refract_r, refract_g, refract_b);

            // Simulated environment reflection via reflection direction
            let reflect_dir = reflect(-v, n);
            let env_color = vec3<f32>(
                0.5 + 0.5 * sin(reflect_dir.x * 3.0 + reflect_dir.y * 2.0),
                0.5 + 0.5 * cos(reflect_dir.y * 2.5 - reflect_dir.z * 1.5),
                0.5 + 0.5 * sin(reflect_dir.z * 2.0 + reflect_dir.x * 3.0)
            );

            // Caustic pattern on the surface
            let caustic = pow(abs(sin(pos.x * 8.0 + pos.y * 6.0) * sin(pos.y * 7.0 - pos.z * 5.0)), 2.0);
            let caustic_color = vec3<f32>(1.0, 1.0, 1.0) * caustic * 0.25;

            // Sharp specular highlights from lights
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let h0 = safe_normalize(l0 + v);
            let spec0 = pow(max(dot(n, h0), 0.0), 256.0) * scene.light0.direction.w;
            let l1 = safe_normalize(-scene.light1.direction.xyz);
            let h1 = safe_normalize(l1 + v);
            let spec1 = pow(max(dot(n, h1), 0.0), 256.0) * scene.light1.direction.w * 0.6;
            let specular = vec3<f32>(1.0, 0.98, 0.95) * (spec0 + spec1) * 1.5;

            // Internal refraction glow (color tint through glass)
            let refract_color = base * 0.15 * chromatic;

            // Combine: reflection + refraction + caustic + specular
            result = env_color * fresnel * 0.5 + refract_color + caustic_color + specular;

            // Depth-based transparency: thin areas more transparent, thick areas less
            let depth = length(pos) * 0.15;
            out_alpha = clamp(0.12 + fresnel * 0.65 + depth * 0.1, 0.08, 0.92);
        }

        // ======== 25: Chrome / Mirror ========
        case 25: {
            let reflect_dir = reflect(-v, n);
            let env_color = vec3<f32>(
                0.5 + 0.5 * reflect_dir.y,
                0.5 + 0.5 * (reflect_dir.x * 0.5 + reflect_dir.y * 0.5),
                0.6 + 0.4 * reflect_dir.y
            );
            let fresnel = pow(1.0 - ndv, 2.0);
            let spec = pow(max(dot(safe_normalize(safe_normalize(-scene.light0.direction.xyz) + v), n), 0.0), 128.0);
            result = env_color * (0.5 + fresnel * 0.5) + vec3<f32>(spec * 2.0);
        }
        // ======== 26: Plastic ========
        case 26: {
            result = three_point_pbr(n, v, base, 0.45, 0.0);
        }
        // ======== 27: Metallic ========
        case 27: {
            result = three_point_pbr(n, v, base, 0.25, 1.0);
        }
        // ======== 28: Velvet ========
        case 28: {
            let rim = pow(1.0 - ndv, 2.0);
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let scatter = max(dot(n, l0), 0.0) * 0.5 + 0.5;
            let velvet = base * scatter * 0.6 + base * rim * 0.8;
            result = velvet + scene.ambient_color.rgb * scene.ambient_color.a * base * 0.2;
        }
        // ======== 29: Holographic ========
        case 29: {
            let angle = dot(n, v);
            let scanline = sin(pos.y * 40.0 + pos.x * 20.0) * 0.5 + 0.5;
            let holo_color = vec3<f32>(
                0.5 + 0.5 * sin(angle * 8.0 + 0.0),
                0.5 + 0.5 * sin(angle * 8.0 + 2.094),
                0.5 + 0.5 * sin(angle * 8.0 + 4.189)
            );
            let edge = pow(1.0 - ndv, 1.5);
            result = holo_color * (0.4 + scanline * 0.3) + vec3<f32>(edge * 0.5);
            out_alpha = 0.6 + edge * 0.3;
        }

        // ================================================================
        // NEW SHADERS 30–41
        // ================================================================

        // ======== 30: Ghost Transparency ========
        case 30: {
            let edge = pow(1.0 - ndv, 2.5);
            let depth = clamp(length(pos) * 0.2, 0.0, 1.0);
            let lit = simple_three_point(n, base * 0.4, 0.3, 32.0);
            let ghost_glow = vec3<f32>(0.6, 0.8, 1.0) * edge * 1.5;
            let inner_glow = vec3<f32>(0.3, 0.5, 0.9) * pow(1.0 - ndv, 6.0) * 0.8;
            result = lit * 0.3 + ghost_glow + inner_glow;
            out_alpha = 0.06 + edge * 0.55 + depth * 0.08;
        }

        // ======== 31: Neon Glow ========
        case 31: {
            let edge = pow(1.0 - ndv, 2.0);
            let neon_r = 0.5 + 0.5 * sin(pos.x * 5.0 + pos.y * 3.0);
            let neon_g = 0.5 + 0.5 * sin(pos.y * 4.0 - pos.z * 2.0 + 1.5);
            let neon_b = 0.5 + 0.5 * cos(pos.z * 3.0 + pos.x * 4.0 + 3.0);
            let neon_color = vec3<f32>(neon_r, neon_g, neon_b);
            let glow = neon_color * (edge * 3.0 + 0.2);
            let core = vec3<f32>(1.0) * pow(edge, 6.0) * 2.0;
            result = glow + core;
            out_alpha = 0.35 + edge * 0.6;
        }

        // ======== 32: Crystal / Diamond ========
        case 32: {
            let reflect_dir = reflect(-v, n);
            let facet = pow(abs(dot(reflect_dir, vec3<f32>(0.577, 0.577, 0.577))), 4.0);
            let facet2 = pow(abs(dot(reflect_dir, vec3<f32>(-0.577, 0.577, -0.577))), 6.0);
            let dispersion = vec3<f32>(
                pow(max(dot(reflect_dir, vec3<f32>(1.0, 0.0, 0.0)), 0.0), 8.0),
                pow(max(dot(reflect_dir, vec3<f32>(0.0, 1.0, 0.0)), 0.0), 8.0),
                pow(max(dot(reflect_dir, vec3<f32>(0.0, 0.0, 1.0)), 0.0), 8.0)
            ) * 4.0;
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let sparkle = pow(max(dot(n, safe_normalize(l0 + v)), 0.0), 128.0) * 3.0;
            let fresnel = pow(1.0 - ndv, 4.0);
            result = vec3<f32>(0.95, 0.97, 1.0) * (facet + facet2) * 0.4 + dispersion + vec3<f32>(sparkle) + vec3<f32>(fresnel * 0.3);
        }

        // ======== 33: Liquid / Water ========
        case 33: {
            let wave1 = sin(pos.x * 6.0 + pos.z * 4.0) * cos(pos.z * 5.0 - pos.x * 3.0);
            let wave2 = sin(pos.y * 8.0 - pos.x * 5.0) * 0.5;
            let wave_normal = n + vec3<f32>(wave1 * 0.08, wave2 * 0.06, wave1 * 0.05);
            let wn = safe_normalize(wave_normal);
            let fresnel = pow(1.0 - max(dot(wn, v), 0.0), 3.0);
            let reflect_dir = reflect(-v, wn);
            let water_env = mix(vec3<f32>(0.0, 0.1, 0.3), vec3<f32>(0.2, 0.5, 0.8), reflect_dir.y * 0.5 + 0.5);
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let caustic = pow(abs(sin(pos.x * 12.0 + wave1 * 4.0) * sin(pos.z * 12.0 + wave2 * 4.0)), 1.5) * 0.4;
            let spec = pow(max(dot(wn, safe_normalize(l0 + v)), 0.0), 64.0) * 0.8;
            result = water_env * (0.4 + fresnel * 0.4) + vec3<f32>(caustic) * vec3<f32>(0.5, 0.8, 1.0) + vec3<f32>(spec);
            out_alpha = 0.45 + fresnel * 0.4;
        }

        // ======== 34: Fire / Lava ========
        case 34: {
            let heat1 = sin(pos.x * 4.0 + pos.y * 6.0) * cos(pos.z * 3.0 - pos.y * 5.0);
            let heat2 = sin(pos.y * 8.0 + pos.x * 3.0 - pos.z * 4.0) * 0.5;
            let heat = heat1 * 0.5 + heat2 * 0.5;
            let heat_map = heat * 0.5 + 0.5;
            let cool = vec3<f32>(0.15, 0.02, 0.0);
            let warm = vec3<f32>(0.8, 0.2, 0.0);
            let hot = vec3<f32>(1.0, 0.6, 0.0);
            let white_hot = vec3<f32>(1.0, 0.95, 0.7);
            var fire_color: vec3<f32>;
            if (heat_map < 0.33) {
                fire_color = mix(cool, warm, heat_map * 3.0);
            } else if (heat_map < 0.66) {
                fire_color = mix(warm, hot, (heat_map - 0.33) * 3.0);
            } else {
                fire_color = mix(hot, white_hot, (heat_map - 0.66) * 3.0);
            }
            let cracks = pow(abs(sin(pos.x * 15.0) * sin(pos.y * 15.0) * sin(pos.z * 15.0)), 0.3);
            let crack_color = mix(fire_color, cool * 0.5, cracks * 0.3);
            let glow = pow(max(heat_map, 0.0), 2.0) * 0.5;
            result = crack_color * (1.0 + glow);
        }

        // ======== 35: Force Field ========
        case 35: {
            let hex1 = sin(pos.x * 10.0) * sin(pos.y * 10.0) * sin(pos.z * 10.0);
            let hex2 = sin(pos.x * 10.0 + 1.57) * sin(pos.y * 10.0 + 1.57) * sin(pos.z * 10.0 + 1.57);
            let hex = pow(abs(hex1) + abs(hex2), 0.5) * 0.5;
            let edge = pow(1.0 - ndv, 2.0);
            let field_color = mix(vec3<f32>(0.0, 0.4, 1.0), vec3<f32>(0.0, 1.0, 0.8), edge);
            let lit = simple_three_point(n, vec3<f32>(0.1, 0.2, 0.4), 0.1, 8.0);
            result = lit + field_color * (edge * 1.5 + hex * 0.3);
            out_alpha = 0.08 + edge * 0.65 + hex * 0.1;
        }

        // ======== 36: Blueprint ========
        case 36: {
            let grid1 = select(0.0, 1.0, fract(pos.x * 2.0) > 0.95 || fract(pos.y * 2.0) > 0.95 || fract(pos.z * 2.0) > 0.95);
            let grid2 = select(0.0, 1.0, fract(pos.x * 10.0) > 0.97 || fract(pos.y * 10.0) > 0.97 || fract(pos.z * 10.0) > 0.97);
            let bg = vec3<f32>(0.05, 0.12, 0.28);
            let line_minor = vec3<f32>(0.15, 0.35, 0.65);
            let line_major = vec3<f32>(0.3, 0.6, 1.0);
            let lit = simple_three_point(n, bg, 0.05, 4.0);
            result = lit + line_minor * grid2 * 0.4 + line_major * grid1 * 0.8;
        }

        // ======== 37: Depth Visualization ========
        case 37: {
            let cam = scene.camera_position.xyz;
            let dist = length(pos - cam);
            let near = 0.5;
            let far_p = 20.0;
            let t = clamp((dist - near) / (far_p - near), 0.0, 1.0);
            let depth_hsv = vec3<f32>(0.66 - t * 0.66, 0.85, 1.0 - t * 0.3);
            result = hsv_to_rgb(depth_hsv);
        }

        // ======== 38: Rainbow Spectrum ========
        case 38: {
            let angle = dot(n, v);
            let hue = fract(angle * 2.0 + length(pos) * 0.1);
            let spectrum = hsv_to_rgb(vec3<f32>(hue, 0.85, 0.9));
            let lit = simple_three_point(n, spectrum, 0.3, 16.0);
            result = lit;
        }

        // ======== 39: Frost / Ice ========
        case 39: {
            let crystal1 = pow(abs(sin(pos.x * 15.0 + pos.y * 8.0) * cos(pos.z * 12.0 - pos.y * 6.0)), 0.5);
            let crystal2 = pow(abs(cos(pos.x * 10.0 - pos.z * 13.0) * sin(pos.y * 11.0 + pos.x * 7.0)), 0.7);
            let frost = (crystal1 + crystal2) * 0.5;
            let fresnel = pow(1.0 - ndv, 3.0);
            let ice_color = mix(vec3<f32>(0.7, 0.85, 1.0), vec3<f32>(0.95, 0.98, 1.0), frost);
            let lit = simple_three_point(n, ice_color * 0.5, 0.6, 64.0);
            let sparkle = pow(max(dot(n, safe_normalize(safe_normalize(-scene.light0.direction.xyz) + v)), 0.0), 128.0);
            result = lit + ice_color * fresnel * 0.4 + vec3<f32>(sparkle * 2.0) * frost;
            out_alpha = 0.5 + fresnel * 0.35 + frost * 0.1;
        }

        // ======== 40: Plasma / Energy ========
        case 40: {
            let p1 = sin(pos.x * 3.0 + pos.y * 5.0) * cos(pos.z * 4.0 - pos.x * 2.0);
            let p2 = sin(pos.y * 6.0 - pos.z * 3.0) * cos(pos.x * 5.0 + pos.z * 2.0);
            let p3 = sin(pos.z * 4.0 + pos.x * 3.0) * cos(pos.y * 5.0 - pos.z * 4.0);
            let plasma = (p1 + p2 + p3) * 0.33 * 0.5 + 0.5;
            let plasma_color = vec3<f32>(
                0.5 + 0.5 * sin(plasma * TWO_PI),
                0.5 + 0.5 * sin(plasma * TWO_PI + 2.094),
                0.5 + 0.5 * sin(plasma * TWO_PI + 4.189)
            );
            let edge = pow(1.0 - ndv, 1.5);
            let energy = pow(plasma, 3.0) * 2.0;
            result = plasma_color * (0.5 + edge * 1.5) + vec3<f32>(energy) * vec3<f32>(0.5, 0.2, 0.8);
            out_alpha = 0.4 + edge * 0.4 + plasma * 0.15;
        }

        // ======== 41: Sketch / Pencil ========
        case 41: {
            let l0 = safe_normalize(-scene.light0.direction.xyz);
            let n_dot_l = max(dot(n, l0), 0.0);
            let p = pos * 12.0;
            let h1 = fract(sin(dot(floor(p.xy), vec2<f32>(12.9898, 78.233))) * 43758.5453);
            let h2 = fract(sin(dot(floor(p.yz), vec2<f32>(45.164, 93.233))) * 23421.631);
            let hatch_dir = fract(p.x + p.y * 0.7);
            let hatch_cross = fract(p.x * 0.7 - p.y + p.z * 0.5);
            let shade = n_dot_l;
            var pencil: f32 = 0.9;
            if (shade < 0.7) { pencil = pencil - smoothstep(0.0, 0.05, abs(hatch_dir - 0.5)) * 0.3 * h1; }
            if (shade < 0.4) { pencil = pencil - smoothstep(0.0, 0.05, abs(hatch_cross - 0.5)) * 0.4 * h2; }
            if (shade < 0.15) { pencil = pencil - 0.3 * h1 * h2; }
            let edge = smoothstep(0.7, 0.95, 1.0 - ndv);
            let paper = vec3<f32>(0.95, 0.93, 0.88);
            let ink = vec3<f32>(0.08, 0.06, 0.04);
            result = mix(paper * pencil, ink, edge * 0.7);
        }

        default: {
            result = simple_three_point(n, base, 0.3, 32.0);
        }
    }

    // Tone mapping (Reinhard) and gamma correction for PBR / HDR modes
    if (mode >= 0 && mode <= 3 || mode == 17 || mode == 18 || mode == 26 || mode == 27 || mode == 32 || mode == 34) {
        result = result / (result + vec3<f32>(1.0)); // Reinhard
        result = pow(result, vec3<f32>(1.0 / 2.2));  // Gamma
    }

    return vec4<f32>(result, out_alpha);
}
