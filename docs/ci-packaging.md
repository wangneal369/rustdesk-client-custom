# GitHub Actions 打包说明

仓库已公开，标准 runner 不计费；仍只保留按需打包，避免每次 push 都跑全平台。

- 入口：`Flutter Tag Build`（`.github/workflows/flutter-tag.yml`）。
  - 推送 `v1.2.3` 形式的 tag：产物发布到同名 Release。
  - Actions 页面手动运行（分支）：产物发布到 `nightly` 预发布。
- 平台：Windows x64（exe/msi）、macOS x86_64 + arm64（dmg）、Linux x64（deb/rpm/Arch 包）、Android arm64（apk，已签名）、iOS arm64（未签名 ipa，需用 AltStore/Sideloadly 等自签安装）；不打任何 32 位产物。
- 关闭：Windows ARM64、Linux aarch64/drm/AppImage/Flatpak、Android armv7/x86_64/universal（矩阵项注释或 `if: false`，需要时恢复）；sciter 版（需 Rust 1.75，与锁定依赖冲突）；Web（上游即关闭）。
- Android 签名来自仓库 secrets `ANDROID_SIGNING_KEY`/`ANDROID_ALIAS`/`ANDROID_KEY_STORE_PASSWORD`/`ANDROID_KEY_PASSWORD`；keystore 原件只保存在维护者本地，丢失后无法覆盖升级已安装的 App。
- `ci`、`flutter-ci`、`flutter-nightly`、`fdroid`、`update-webpki-roots` 的自动触发已移除，只能手动运行。
- 未配置签名密钥时，Windows / macOS 签名步骤自动跳过。
