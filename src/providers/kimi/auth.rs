use crate::providers::common::cli;

pub(super) fn get_token() -> Option<String> {
    std::env::var("KIMI_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

/// GUI 启动的进程 PATH 很小，裸 `Command::new` 会把装好的 kimi 判成未安装，
/// 于是提示用户去装 CLI，而真正缺的是 `KIMI_AUTH_TOKEN`。
pub(super) fn kimi_cli_exists() -> bool {
    cli::command_exists("kimi")
}
