use std::path::Path;

pub fn get_foreground_process_name() -> Option<String> {
    let window = active_win_pos_rs::get_active_window().ok()?;
    Path::new(&window.process_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}
