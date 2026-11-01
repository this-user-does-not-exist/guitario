use guitario_engine::Session;
use guitario_ui_egui::GuitarioApp;

fn main() -> eframe::Result {
    // Each file named on the command line opens in a tab of its own (Windows and Linux pass the
    // files chosen in the file manager this way).
    let mut files: Vec<String> = std::env::args().skip(1).collect();
    // `--control <port>` (or GUITARIO_CONTROL) serves the control protocol on 127.0.0.1 so agents
    // and scripts can drive the app (docs/control.md); 0 picks a free port.
    let mut control = std::env::var("GUITARIO_CONTROL").ok();
    if let Some(i) = files.iter().position(|a| a == "--control") {
        files.remove(i);
        if i < files.len() {
            control = Some(files.remove(i));
        }
    }
    let control = match control.as_deref().map(str::parse::<u16>) {
        Some(Ok(port)) => Some(port),
        Some(Err(_)) => {
            eprintln!("--control takes a port number such as 4747");
            std::process::exit(2);
        }
        None => None,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Guitario")
            // Matches the desktop entry, so Linux desktops show its icon and file types.
            .with_app_id("ai.storyteller.guitario")
            .with_icon(guitario_ui_egui::app_icon::window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Guitario",
        options,
        Box::new(move |cc| {
            // Preferences, recent files, autosave and crash recovery all live behind the store.
            let (store, report) = GuitarioApp::open_settings();
            let mut app = GuitarioApp::new(Session::new(), None).with_default_sounds().with_settings(store, &report).with_sound_offer();
            app.setup(&cc.egui_ctx);
            if let Some(port) = control {
                match app.listen(port) {
                    Ok(addr) => eprintln!("Guitario control channel on {addr}"),
                    Err(e) => eprintln!("could not open the control channel on port {port}: {e}"),
                }
            }
            for path in &files {
                app.open_file(path);
            }
            // Without a file to open, Guitario starts on its Homepage, as Guitar Pro does.
            if files.is_empty() {
                app.show_home();
            }
            Ok(Box::new(app))
        }),
    )
}
