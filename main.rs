// main.rs — TitanDAW v2   JCH-2026-011
mod ffi;
mod engine;
mod midi;
mod brain;
mod session;
mod ui;
mod content_ui;

use anyhow::Result;
use gtk4::prelude::*;
use gtk4::Application;

const APP_ID: &str = "com.titanuai.titandaw";

fn main() -> Result<()> {
    env_logger::init();
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        ui::build_and_show(app);
    });
    app.run();
    Ok(())
}
