#![windows_subsystem = "windows"]

mod app;
mod camera;
mod color_scheme;
mod export;
mod mesh;
mod render;
mod ui;

use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    // 窗口属性（标题/尺寸）在 App::resumed 中定义，见 app.rs
    let mut app = pollster::block_on(crate::app::App::new());

    event_loop.run_app(&mut app).unwrap();
}
