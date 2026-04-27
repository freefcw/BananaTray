fn main() {
    // 获取 Git 短 Hash，失败时设为 "unknown"
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=BANANATRAY_GIT_HASH={hash}");

    // 仅在 HEAD 变化时重新运行（避免每次编译都执行 git）
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");
    emit_locale_rerun_directives();
}

fn emit_locale_rerun_directives() {
    println!("cargo:rerun-if-changed=locales");

    let Ok(entries) = std::fs::read_dir("locales") else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_locale_file = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "yml" | "yaml"));
        if is_locale_file {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
