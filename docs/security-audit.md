# 审计补强说明（2026-10-24）

## 配置与行为

- 复用既有管理配置中的 `api-server` 作为实际 HTTPS API 基地址，并设置 `audit-token`；本补丁不新增设置 UI。配套 hbbs 的 `AUDIT_TOKEN` 必须相同，`AUDIT_LISTEN` 建议绑定 `127.0.0.1:21114` 并由 HTTPS 网关代理 `/api/audit/conn`、`file`、`alarm`。默认不自动开启集中审计。
- 远端审计请求必须 HTTPS、校验证书、不跟随重定向，配置代理失败不绕过；仅字面量 loopback IP 允许 HTTP。自签名服务应安装可信证书，不允许通过无效证书兼容回退绕过校验。
- 连接、文件、告警记录先写主配置旁的 `.audit-outbox`，保留 nonce 与事件时间。后台及重启扫描补传，只有空 2xx 响应才确认删除；失败、损坏文件、旧 URL 记录均保留。不会向历史服务地址发送新 token。
- 文件审计按每批 10 项记录全部名称，包含批次标识与总数，不再截断为前 10 项。与既有第三方管理 API 联用时，须验证认证、批次、nonce 和响应兼容性。
- outbox 不存 Bearer token，但 token 本身复用本地配置存储。配置、outbox、日志均须限制 ACL，监控剩余空间并规划归档；尚未增加 token 配置 UI 或秘密管理平台。

## 验证范围

```powershell
cargo test --locked -p base --all-targets -- --include-ignored
```

其中基础库测试验证 URL 边界、重启恢复、补传、失败保留、旧 URL 隔离、完整文件批次；`libs/base/tests/audit_http.rs` 直接编译实际 `src/hbbs_http/http_client.rs` 并验证禁止重定向及远端明文。不是用假网络实现代替实际 HTTP 工厂。

这不等于完整客户端 / Flutter / 原生媒体依赖或生产 TLS 网关验证通过。服务端安全记录、精确测试名称及剩余依赖命中见配套服务端仓库的 `docs/security-hardening.md`。

## 明确边界

- 公钥匹配不等于人员身份认证；原有密码、人工确认、可选 2FA、白名单和会话权限链路继续保留。未实现企业 IAM / RBAC / 离职撤权。
- 共享审计 token 不能证明设备声明或人员身份不可伪造。异步事件进入落盘任务之前仍存在崩溃窗口；不是全过程零丢失或端到端 exactly-once 保证。
- 普通历史 HTTP 接口的无效证书兼容模式未在本补丁中全局更改。RustSec 固定快照扫描本仓库锁文件仍有 12 条“版本 × 公告”命中（包含其他平台/构建依赖），不是漏洞清零。
- 品牌、logo、安装包签名和全平台发布尚未在本补丁中实现；不得删除许可证及必要开源声明。
