# GitHub Actions 打包说明

仓库已公开，标准 runner 不计费；仍只保留按需打包，避免每次 push 都跑全平台。

- 入口：`Flutter Tag Build`（`.github/workflows/flutter-tag.yml`）。
  - 推送 `v1.2.3` 形式的 tag：产物发布到同名 Release。
  - Actions 页面手动运行（分支）：产物发布到 `nightly` 预发布。
- 平台：只打常用桌面平台——Windows x64（exe/msi）、macOS x86_64 + arm64（dmg）、Linux x64（deb/rpm/Arch 包）；不打任何 32 位产物。
- 关闭：Windows ARM64、Linux aarch64/drm/AppImage/Flatpak、Android（小众，矩阵项注释或 `if: false`，需要时恢复）；sciter 版（需 Rust 1.75，与锁定依赖冲突）；iOS（上游不发布 ipa）；Web（上游即关闭）。
- `ci`、`flutter-ci`、`flutter-nightly`、`fdroid`、`update-webpki-roots` 的自动触发已移除，只能手动运行。
- 未配置签名密钥时，Windows / macOS 签名步骤自动跳过。
