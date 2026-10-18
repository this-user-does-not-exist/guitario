use guitario_engine::Session;
use guitario_ui_egui::GuitarioApp;
use serde_json::json;

fn main() -> eframe::Result {
    let mut session = Session::new();
    let mut startup_error = None;
    let file = std::env::args().nth(1);
    if let Some(path) = file.clone()
        && let Err(e) = session.run("file.open", &json!({"path": path}))
    {
        startup_error = Some(e.to_string());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Guitario")
            .with_icon(guitario_ui_egui::app_icon::window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Guitario",
        options,
        Box::new(move |cc| {
            // Preferences, recent files, autosave and crash recovery all live behind the store.
            let (store, report) = GuitarioApp::open_settings();
            let mut app = GuitarioApp::new(session, startup_error).with_default_sounds().with_settings(store, &report);
            app.setup(&cc.egui_ctx);
            // Without a file to open, Guitario starts on its Homepage, as Guitar Pro does.
            if file.is_none() {
                app.show_home();
            }
            Ok(Box::new(app))
        }),
    )
}
