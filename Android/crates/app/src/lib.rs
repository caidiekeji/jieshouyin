// 移动端库入口：Tauri Android 以 cdylib 形式加载，需带 mobile_entry_point 宏。
mod commands;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    commands::run();
}
