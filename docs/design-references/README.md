# 设计参考素材

本目录保存**不会被打包**进发行版（macOS .app / Linux deb/rpm/AppImage）的设计资料，
避免污染 `src/icons/` 并把无用资源塞进安装包。

## 当前资料

- `app_logo_gauge.png` — App Logo 的 gauge 风格（**当前正在使用**，已复制为 `src/icons/app_logo.png`）
- `app_logo_peeling.png` — App Logo 的 peeling 风格备选
- `app_logo_original.png` — 早期使用的初版 logo（备份保留）

正在使用的 logo 是 [`src/icons/app_logo.png`](../../src/icons/app_logo.png)，
内容与 `app_logo_gauge.png` 一致。如需切换风格，把对应文件复制覆盖
`src/icons/app_logo.png` 即可。

## 添加新设计资料

如果某个 PNG / 草图只是作为参考保存，**不要**放在 `src/icons/`：
打包脚本 `scripts/common.sh` 的 `copy_runtime_resources()` 用 `cp src/icons/*.png`
通配复制，会把所有 PNG 塞进发行包；其中 `app_logo.png` 是必需运行时资源，
缺失会直接中止打包。请放在本目录或 `docs/` 下其他子目录。
