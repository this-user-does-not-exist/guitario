use guitario_engine::Session;
use guitario_ui_egui::GuitarioApp;
use serde_json::json;

fn main() -> eframe::Result {
    let mut session = Session::new();
    let mut startup_error = None;
    if let Some(path) = std::env::args().nth(1)
        && let Err(e) = session.run("file.open", &json!({"path": path}))
    {
        startup_error = Some(e.to_string());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]).with_min_inner_size([900.0, 600.0]).with_title("Guitario"),
        ..Default::default()
    };
    eframe::run_native(
        "Guitario",
        options,
        Box::new(move |cc| {
            let mut app = GuitarioApp::new(session, startup_error).with_default_sounds();
            app.setup(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
