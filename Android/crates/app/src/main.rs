// Android 构建复用同一入口：windows_subsystem 仅在 Windows 生效，移动端忽略。

mod commands;
mod state;

fn main() {
    commands::run();
}
