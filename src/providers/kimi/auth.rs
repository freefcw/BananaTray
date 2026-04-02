pub(super) fn get_token() -> Option<String> {
    std::env::var("KIMI_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

pub(super) fn kimi_cli_exists() -> bool {
    std::process::Command::new("kimi")
        .arg("--version")
        .output()
        .is_ok()
}
