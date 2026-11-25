// 桌面二进制入口复用 lib.rs 的 run()，避免 commands/state 模块编译两份。
fn main() {
    checkout_app::run();
}
