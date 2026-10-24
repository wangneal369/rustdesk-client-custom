# 安全功能范围（2026-10-24）

按用户确认的用途，本定制版本不建设平台账号、JWT、后台面板或集中操作审计。保留公钥验签、连接加密、既有被控端密码/人工确认/可选 2FA、白名单及会话权限校验；公钥匹配不等于人员身份认证。

## 本次收敛

- 删除前阶段新增的 audit outbox、重启补传、审计专用 HTTP 工厂、audit-token 选项和文件审计分批。相关启动钩子、导出、依赖及专用测试一并删除。
- 共用 `get_audit_server` 固定返回空地址，阻断自建 API 配置自动产生审计出口。上游连接/文件/告警上传与会话备注 UI 沿用原有空地址检查，不再产生有效审计目标。
- 备注菜单只在存在审计出口时显示，避免无用的登录提示；原生 `send_note` 空地址直接返回。手写 Web bridge 的审计出口及备注发送同时关闭；不运行 bridge codegen。
- 自动心跳 `heartbeat_url` 仅在用户显式配置 `api-server`（且不是 rustdesk.com 公共域名）时向 `<api-server>/api/heartbeat` 发送；不再从自建 ID 服务器推导心跳地址（原先会推导出明文 `http://<host>:21114`）。未配置 `api-server` 时心跳关闭，不上报设备信息/活动连接，也不接受 strategy 覆盖或 disconnect 指令；配置后，这些行为仅对该显式指定的 API 服务器生效。
- 上游其他可选管理 API / 登录界面不做大范围删除；本项目不实现它们对应的账号后端，也不将其计为交付能力。
- 现有 `.audit-outbox` 文件与配置不会被自动清理，但不再扫描或补传；维护者可自行归档。不会在代码删减时删除用户数据。

## 回归面

- `src/common.rs`：集中审计出口关闭，包含配置过 API / rendezvous 的回归用例。
- `src/hbbs_http/sync.rs`：仅 `heartbeat_url` 改为读取显式 `api-server`，未配置时沿用原有空地址分支（不上传、不处理响应）；用户主动的登录、地址簿、`--assign`、切换授权等路径不变。
- `src/server.rs`、`src/server/connection.rs`：撤去新增审计 worker / outbox 调用，恢复前阶段审计实现；实际出口由上述空地址策略关闭，不修改远控授权、媒体或文件传输处理。
- `src/hbbs_http.rs`、`src/hbbs_http/http_client.rs`：撤去审计专用工厂及为其引入的共享改写，恢复原有普通/严格 HTTP 工厂，不扩大其他路径的变更面。
- `src/ui_session_interface.rs`、`flutter/lib/common/widgets/toolbar.dart`、`flutter/lib/web/bridge.dart`：关闭无出口的备注请求、入口及 Web 对应桥接；其他会话操作不变。
- `libs/base`：撤去审计专用模块、选项和依赖，保留文件路径安全、覆盖确认、软/硬链接及中断恢复等既有测试。

基础库验证命令为 `cargo test --locked --offline -p base --all-targets -- --include-ignored`。集中审计出口回归在 `common::tests::centralized_audit_stays_disabled_with_configured_api_servers`；本机从实际源码单独编译纯函数和该测试，不声称完整客户端构建通过。

本机无 Dart / Flutter SDK，新增 UI 适配仅经过源码复查，未通过分析器、组件或浏览器验证；Web JS 核心不在本仓库，未验证其内部实现。完整客户端 / 原生媒体链仍缺 VCPKG 配套依赖，生产端到端联调、全平台发布和品牌替换未完成。配套服务端的日志轮转、包长限制、依赖修补及审计代码删减说明见其 `docs/security-hardening.md`。
