# GitHub Actions 打包说明

仓库已公开，标准 runner 不计费；仍只保留按需打包，避免每次 push 都跑全平台。

- 入口：`Flutter Tag Build`（`.github/workflows/flutter-tag.yml`）。
  - 推送 `v1.2.3` 形式的 tag：产物发布到同名 Release。
  - Actions 页面手动运行（分支）：产物发布到 `nightly` 预发布。
- 平台：Windows x64/ARM64、macOS x86_64/arm64、Linux x64/aarch64（deb/rpm/AppImage/Flatpak/drm）、Android（arm64/armv7/x86_64）。
- 关闭：sciter 版 Windows/Linux（sciter 需 Rust 1.75，锁定依赖需 ≥ 1.85）；Android universal（需 `ANDROID_SIGNING_KEY` 等签名 secrets）；iOS（上游本就不发布 ipa，需 Apple 签名）；Web（上游即关闭）。
- 未配置 Android 签名时只发布未签名 APK，手机无法直接安装。
- `ci`、`flutter-ci`、`flutter-nightly`、`fdroid`、`update-webpki-roots` 的自动触发已移除，只能手动运行。
- 未配置签名密钥时，Windows / macOS 签名步骤自动跳过。
