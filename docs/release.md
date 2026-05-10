# Release

BananaTray 的 GitHub Release 采用“自动构建草稿，人工最终发布”的流程。

## 发布语义

- 版本来源是 Git tag，格式为 `vX.Y.Z` 或带预发布后缀的 `vX.Y.Z-...`。
- tag 去掉前缀 `v` 后必须和 `Cargo.toml` 的 `[package].version` 一致。
- 推送 tag 后，GitHub Actions 会创建或更新同名 draft release。
- workflow 只上传构建产物和平台校验文件，不会自动 publish。
- release workflow 为了控制 Actions 用量，不重复运行完整 lint/test；质量门禁依赖常规 CI、App CI 和发布前本地验证。
- 维护者需要在 GitHub Release 页面检查 release notes、产物和校验值后手动点击 **Publish release**。

## Actions 成本边界

Release workflow 只负责“构建可下载产物 + 写入 draft release”，不负责替代 CI：

- PR / branch push 仍由 `ci.yml` 跑低成本 GPUI-free 门禁。
- 完整 app 检查仍由 `app-ci.yml` 手动或定时运行。
- tag release 不再额外跑 clippy / test，避免在已经构建 release 二进制的基础上重复占用 runner 时间。
- Linux 和 macOS release job 并行执行；macOS 使用 matrix 同时构建 aarch64（Apple Silicon）和 x86_64（Intel）两个架构。各 job 完成后直接上传本平台产物到同一个 draft release，不再额外开汇总发布 job。

## 自动产物

`.github/workflows/release.yml` 会构建并上传：

- Linux `.deb`
- Linux `.rpm`
- Linux `.AppImage`
- GNOME Shell Extension `.zip`
- macOS `.dmg`（aarch64 + x86_64 双架构）
- macOS 裸二进制 tarball（aarch64 + x86_64 双架构）
- `SHA256SUMS-linux`
- `SHA256SUMS-macos-aarch64` / `SHA256SUMS-macos-x86_64`

Linux deb/rpm 使用仓库里的 `scripts/bundle-deb.sh` 和 `scripts/bundle-rpm.sh`，会包含 D-Bus activation 与 systemd user service。AppImage 会移除这些宿主级 activation 文件。GNOME Shell Extension 作为独立 zip 产物发布，不随 deb/rpm 自动写入系统扩展目录。

## 发布步骤

1. 确认工作树干净，并把 `Cargo.toml` 的版本号更新到目标版本。
2. 运行本地验证：

```bash
cargo fmt --check
./scripts/check-gpui-imports.sh
./scripts/check-gnome-extension.sh
cargo clippy --lib --no-default-features -- -D warnings
cargo test --lib --no-default-features
```

3. 提交版本号和对应变更。
4. 创建并推送 tag：

```bash
git tag v0.1.0
git push origin v0.1.0
```

5. 等待 `Release` workflow 完成。
6. 打开 GitHub draft release，检查：
   - release notes 是否准确
   - Linux 和 macOS（aarch64 + x86_64）产物是否齐全
   - `SHA256SUMS-linux` / `SHA256SUMS-macos-aarch64` / `SHA256SUMS-macos-x86_64` 是否覆盖对应平台产物
   - 预发布版本是否被标记为 prerelease
7. 确认无误后手动 publish。

## 重新生成草稿

如果某次 release workflow 失败，修复后可以重新运行失败的 job。也可以在 Actions 页面手动触发 `Release` workflow，并填写已有 tag，例如 `v0.1.0`。

不要为同一个已发布版本移动 tag。需要替换已经 publish 的版本时，优先发布新的补丁版本。
