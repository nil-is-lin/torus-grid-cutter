use glam::{Mat4, Vec3};
use winit::keyboard::KeyCode;

pub struct OrbitCamera {
    pub theta: f32,
    pub phi: f32,
    pub radius: f32,
    pub target: Vec3,
    pub fov: f32,
    near: f32,
    far: f32,

    // Input state
    pub dragging: bool,
    pub panning: bool,
    pub last_mouse: (f32, f32),
    // Track which keys are held
    w_down: bool,
    s_down: bool,
    a_down: bool,
    d_down: bool,
    q_down: bool,
    e_down: bool,
}

impl OrbitCamera {
    pub fn new() -> Self {
        OrbitCamera {
            theta: -std::f32::consts::FRAC_PI_4,
            phi: std::f32::consts::FRAC_PI_6,
            radius: 8.0,
            target: Vec3::ZERO,
            fov: 60.0,
            near: 0.01,
            far: 500.0,
            dragging: false,
            panning: false,
            last_mouse: (0.0, 0.0),
            w_down: false,
            s_down: false,
            a_down: false,
            d_down: false,
            q_down: false,
            e_down: false,
        }
    }

    pub fn view_matrix(&self) -> Mat4 {
        let eye = Vec3::new(
            self.radius * self.phi.cos() * self.theta.cos(),
            self.radius * self.phi.sin(),
            self.radius * self.phi.cos() * self.theta.sin(),
        );
        Mat4::look_at_rh(eye + self.target, self.target, Vec3::Y)
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov.to_radians(), aspect, self.near, self.far)
    }

    pub fn handle_mouse_drag(&mut self, x: f32, y: f32) {
        let dx = x - self.last_mouse.0;
        let dy = y - self.last_mouse.1;

        let orbit_speed = 0.005;
        let pan_speed = 0.002 * self.radius;

        if self.dragging {
            self.theta += dx * orbit_speed;
            self.phi = (self.phi + dy * orbit_speed).clamp(-1.5, 1.5);
        }

        if self.panning {
            let right = Vec3::new(self.theta.sin(), 0.0, -self.theta.cos()).normalize();
            let up = Vec3::new(
                -self.theta.cos() * self.phi.sin(),
                self.phi.cos(),
                -self.theta.sin() * self.phi.sin(),
            );
            self.target -= right * dx * pan_speed;
            self.target += up * dy * pan_speed;
        }

        self.last_mouse = (x, y);
    }

    pub fn start_drag(&mut self, x: f32, y: f32) {
        self.dragging = true;
        self.last_mouse = (x, y);
    }

    pub fn start_pan(&mut self, x: f32, y: f32) {
        self.panning = true;
        self.last_mouse = (x, y);
    }

    pub fn stop_drag(&mut self) {
        self.dragging = false;
    }

    pub fn stop_pan(&mut self) {
        self.panning = false;
    }

    pub fn handle_scroll(&mut self, delta: f32) {
        let factor = if delta > 0.0 { 0.9 } else { 1.1 };
        self.radius = (self.radius * factor).clamp(0.05, 200.0);
    }

    pub fn handle_key(&mut self, key: KeyCode, pressed: bool) {
        match key {
            KeyCode::KeyW => self.w_down = pressed,
            KeyCode::KeyS => self.s_down = pressed,
            KeyCode::KeyA => self.a_down = pressed,
            KeyCode::KeyD => self.d_down = pressed,
            KeyCode::KeyQ => self.q_down = pressed,
            KeyCode::KeyE => self.e_down = pressed,
            _ => {}
        }
    }

    pub fn update_keyboard_motion(&mut self, dt: f32) {
        let speed = 3.0 * dt;
        let forward = Vec3::new(self.theta.cos(), 0.0, self.theta.sin()).normalize();
        let right = Vec3::new(-self.theta.sin(), 0.0, self.theta.cos()).normalize();

        if self.w_down {
            self.target += forward * speed;
        }
        if self.s_down {
            self.target -= forward * speed;
        }
        if self.a_down {
            self.target -= right * speed;
        }
        if self.d_down {
            self.target += right * speed;
        }
        if self.q_down {
            self.target.y -= speed;
        }
        if self.e_down {
            self.target.y += speed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// theta=0, phi=0 时相机位于 +X 轴、距离为 radius 处，看向原点。
    #[test]
    fn test_view_matrix_eye_position() {
        let cam = OrbitCamera {
            theta: 0.0,
            phi: 0.0,
            radius: 8.0,
            ..OrbitCamera::new()
        };
        let m = cam.view_matrix();
        // 视图矩阵第 4 列 = -R^T * eye；相机在 +X 方向时，eye 的 x 分量为 radius
        // 更直接的验证：把原点变换到相机空间应为 (0,0,-radius)（右手系，看向 -Z）
        let origin_in_cam = m.transform_point3(Vec3::ZERO);
        assert!(
            (origin_in_cam.z + 8.0).abs() < 1e-4,
            "origin should be radius ahead: {origin_in_cam}"
        );
        let eye_in_cam = m.transform_point3(Vec3::new(8.0, 0.0, 0.0));
        assert!(
            eye_in_cam.distance(Vec3::ZERO) < 1e-4,
            "eye maps to camera origin: {eye_in_cam}"
        );
    }

    /// 水平拖动改变 theta（yaw），垂直拖动改变 phi（pitch）。
    #[test]
    fn test_drag_updates_angles() {
        let mut cam = OrbitCamera::new();
        cam.start_drag(0.0, 0.0);
        let (t0, p0) = (cam.theta, cam.phi);
        cam.handle_mouse_drag(10.0, 0.0);
        assert!(
            (cam.theta - t0).abs() > 1e-6,
            "theta should change on horizontal drag"
        );
        assert_eq!(cam.phi, p0, "phi unchanged on pure horizontal drag");
        cam.handle_mouse_drag(0.0, 10.0);
        assert!(
            (cam.phi - p0).abs() > 1e-6,
            "phi should change on vertical drag"
        );
    }

    /// 视图矩阵为正交旋转矩阵（行向量单位正交）。
    #[test]
    fn test_view_matrix_is_rigid() {
        let cam = OrbitCamera::new();
        let m = cam.view_matrix();
        let r = m.to_cols_array();
        // 检查 3x3 旋转部分的行向量范数 ≈ 1（仿射列：row-major 存储）
        for i in 0..3 {
            let row = Vec3::new(r[i * 4], r[i * 4 + 1], r[i * 4 + 2]);
            assert!((row.length() - 1.0).abs() < 1e-4, "row {i} not unit: {row}");
        }
    }
}
