# Android sidecar 分支

`android-sidecar` 分支把 CC Switch 编译成 Android aarch64 sidecar，供 [ZeroTermux-CCS](https://github.com/AGAGAG666/ZeroTermux-CCS) 内嵌使用。前端仍以 CC Switch React 页面为主体，Android 专用代码只补齐 Tauri API 替身、WebView 宿主能力、移动端布局和 sidecar 运行环境。

本分支不是上游 CC Switch 的官方发布分支，也没有得到上游作者背书。原版上游为 [farion1231/cc-switch](https://github.com/farion1231/cc-switch)，当前对齐版本为 `v3.20.0`，许可证保持 MIT 不变。

## 适配结构

| 部分 | 做法 |
| --- | --- |
| Rust 后端 | 用 `src-tauri/android/` shim 替代 Tauri runtime 和部分插件，复用 CC Switch Rust 核心 |
| Android sidecar | `src-tauri/src/sidecar.rs` 提供 HTTP、RPC、SSE 和 Web 静态资源服务 |
| React 前端 | Android 构建时把 `@tauri-apps/*` alias 到 `src/android-bridge/` |
| 移动端 UI | 只在 Android 构建注入 `src/android-bridge/mobile.css`，必要时通过 scoped class 标记目标组件 |
| Android 宿主 | ZeroTermux 负责 WebView、目录选择、Intent、前台服务和 sidecar 生命周期 |

桌面构建继续使用真实 Tauri API；Android alias 和移动 CSS 只在 `CCS_TARGET=android` 时生效。

## Sidecar 服务

Android 下 `src-tauri/src/main.rs` 调用 `run_sidecar`：

1. 监听回环地址上的运行端口；
2. 使用 `--webroot` 托管 `ccs-web.zip` 解包后的静态文件；
3. 通过 stdout 输出端口与 token；
4. 在 `index.html` 的 `<head>` 注入 `window.__CCS_SIDECAR__`；
5. 用 `/rpc/{command}` 和 `/events` 提供命令与 SSE。

Web 页面、RPC 和 SSE 同源。ZeroTermux 默认把 Web 服务配置为 `17132`，也允许用户修改；handshake 中的实际端口始终是前端依据。Codex/OpenCode 的路由代理端口为 `15721`，它与 Web 端口是两项独立配置。认证使用 `x-ccs-token`，SSE 使用 `?token=`；不要改成通用 `Authorization: Bearer`。

## Android bridge

主要入口：

- `src/android-bridge/runtime.ts`：handshake、RPC、SSE 和宿主动作；
- `src/android-bridge/deeplink.ts`：从 ZeroTermux 一次性取回待处理 deep link；
- `src/components/DeepLinkImportDialog.tsx`：解析结果展示、风险确认和导入；
- `src/android-bridge/mobile.css`：Android 窄屏和触控覆盖层。

需要 Android `Context` 的动作由宿主处理，Rust sidecar 不直接启动 Activity。目录选择由 ZeroTermux 的真实路径选择器完成，不使用只返回 `content://` 的 SAF 树 URI。

### `ccswitch://v1/import`

ZeroTermux 只接收精确的 `ccswitch://v1/import`，通过不含原始 URI 的 JavaScript 通知让前端调用 `takePendingDeepLink()`。前端随后使用现有 `parse_deeplink` Rust 命令，并继续显示 CC Switch 原有确认弹窗。

provider 链接可能在 query 中携带 `apiKey`：

- 不记录完整 URI；
- 不把真实 key 写进测试、README、Issue 或提交；
- 测试使用 `sk-test-placeholder` 和 `example.test`；
- 已公开的 key 必须轮换。

## Skills 移动端布局

Android 触控规则会把图标按钮扩大到可点击尺寸，桌面单行布局可能因此把 Skill 名称和描述挤到零宽。当前实现使用 Skills 专用 class：

- 顶部操作按钮允许换行；
- 应用计数标签在卡片内换行；
- 名称和描述占满第一行；
- 应用开关和删除按钮放在下一行。

不要用影响所有 `.flex` 或所有列表项的全局覆盖替代这些 scoped 规则。

## Rust shim workspace

`src-tauri/android/` 是独立 workspace，包含 Tauri、插件、剪贴板、自动启动和宏替身，以及 `shim-selftest`。其中 `Cargo.lock` 是本地 Cargo 自动生成物，当前不入库；CI 没有使用 `--locked`。需要时由 Cargo 重新生成，不要把本地 Lock 或 `target/` 当作发布资产。

命令注册的单一来源是 `src-tauri/src/command_list.rs`。新增或修改命令时必须同时验证桌面 Tauri 和 Android RPC 参数名，不能维护两份手工命令表。

## 构建与测试

```bash
pnpm typecheck:android
pnpm test:unit
pnpm build:android

cd src-tauri
cargo test --test deeplink_import
```

本地 `dist-android/` 只是可重新生成的构建输出，不提交、不作为正式资产。正式产物由 GitHub Actions 生成：

| 工作流 | 使用场景 |
| --- | --- |
| `android-frontend-build.yml` | React/bridge/CSS 修改，生成 `ccs-android-dist` |
| `android-sidecar-build.yml` | Rust sidecar 或命令面修改，生成 `ccs-sidecar_arm64-v8a` |
| `android-shim-probe.yml` | `src-tauri/android/` shim 或宏修改 |
| `android-proxy-tests.yml` | 代理、协议转换、转发或流式响应修改 |

只要求覆盖本次改动范围的工作流，但需要新 ELF 时必须运行 Sidecar Build。

## 当前发布基线

发布来源：

- Branch：`android-sidecar`
- 资产来源提交：`f3291ac67659784ed39288828a944d68e7c39abb`

资产 Release：`ccs-android-f3291ac6`

| 文件 | 大小 | SHA-256 |
| --- | ---: | --- |
| `ccs-web.zip` | 2,080,953 bytes | `5b446fa6eda7ab209b7ffa5dabb731b363a8dfd55fc82f56ba9b459d362b021b` |
| `libccsidecar.so` | 17,133,944 bytes | `668330a171b699704c380ae979d146d9ae9d99ecb630fca05fa17d922af040f4` |

成功 Run：

- Android Frontend Build：`37899238675`；
- Android Sidecar Build：`37899238650`。

消费该资产的 ZeroTermux-CCS 正式 Release 为 `ccs-f3291ac6`。真机已确认 `ccswitch://v1/import` 可用，Skills 管理页窄屏显示正常。

## 与 ZeroTermux 的发布关系

1. 在 `android-sidecar` 完成修改、测试和推送；
2. 由 Actions 生成 Web 与 sidecar；
3. 同一 `ccs-android-<commit>` Release 同时发布 `ccs-web.zip` 和 `libccsidecar.so`；
4. ZeroTermux `app/build.gradle` 锁定 Tag、size、SHA-256；
5. ZeroTermux Actions 构建 APK；
6. 从 APK 解出两个资产，与 Release 和 Gradle 三方比对；
7. 手机回归通过后再发布 ZeroTermux-CCS Release。

完整确认闸门见本地外部知识库 `~/工作区/codex/Termux/docs/build-and-release.md`。仓库内文档和外部交接文档是两个范围；收到笼统的“更新文档”请求时必须先确认目标范围。

## 同步上游

```bash
git fetch origin --prune
git merge-base origin/main android-sidecar
git diff --stat $(git merge-base origin/main android-sidecar)..android-sidecar
```

同步上游后重新检查 Android alias、shim、命令表、移动 CSS 和 deep-link 接缝，不根据旧测试数量判断完成。
