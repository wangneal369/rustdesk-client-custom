# GitHub Actions 打包说明

仓库为私有，Actions 按分钟计费（Free 计划每月 2000 分钟；Windows ×2，macOS ×10），因此只保留按需打包。

- 入口：`Flutter Tag Build`（`.github/workflows/flutter-tag.yml`）。
  - 推送 `v1.2.3` 形式的 tag：产物发布到同名 Release。
  - Actions 页面手动运行（分支）：产物发布到 `nightly` 预发布。
- 平台：Windows x64、Linux x64（deb/rpm）、macOS arm64（dmg，未签名）。
- 其余平台（iOS、Android、Windows ARM64、macOS x86_64、Linux aarch64/drm/sciter/AppImage/Flatpak）在 `flutter-build.yml` 中以 `if: false` 或注释掉矩阵项关闭，需要时恢复即可。
- `ci`、`flutter-ci`、`flutter-nightly`、`fdroid`、`update-webpki-roots` 的自动触发已移除，只能手动运行。
- 未配置签名密钥时，Windows / macOS 签名步骤自动跳过。
