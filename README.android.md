# Android sidecar 分支

这个分支把 CC Switch 编译成 Android aarch64 sidecar，供
[ZeroTermux-CCS](https://github.com/AGAGAG666/ZeroTermux-CCS) 内嵌使用。
前端仍是 CC Switch 的 React 页面，Android 宿主负责启动 sidecar、提供 WebView 和少量系统能力。

本分支不是上游 CC Switch 的官方发布分支，也没有得到上游作者背书。上游仓库是
[farion1231/cc-switch](https://github.com/farion1231/cc-switch)，许可证保持 MIT 不变。

## 适配方式

Android 版没有桌面 Tauri 窗口。适配集中在两层：

| 部分 | 做法 |
| --- | --- |
| Rust 后端 | 用 Android shim 替代 Tauri 及部分插件，核心业务代码尽量保持上游结构 |
| React 前端 | 在 Android 构建中把 `@tauri-apps/*` alias 到 `src/android-bridge/` |

少数需要跟随上游 API 的地方放在 Android 条件编译、sidecar 入口和统一命令表中。这样同步上游时，主要处理这些接缝，不必重写整套业务逻辑。

## Rust 侧

`src-tauri/android/` 是独立 workspace，包含以下替身和测试 crate：

| crate | 用途 |
| --- | --- |
| `tauri-shim` | 提供 `app`、`plugins`、`rpc`、`runtime`、`state`、`window` 等接口 |
| `tauri-shim-macros` | 提供 `#[command]` 和 `generate_handler!` 的 Android 实现 |
| `arboard-shim` | 把剪贴板操作转交给 Termux |
| `auto-launch-shim` | Android 上的开机启动占位实现 |
| `tauri-plugin-*` shim | 对话框、打开外部路径、存储和更新接口 |
| `shim-selftest` | 用上游真实命令签名测试 shim 行为 |

v3.20.0 的桌面和 Android sidecar 共用 303 条命令。命令表只有一份，位于
`src-tauri/src/command_list.rs`。

### sidecar 启动

`src-tauri/src/main.rs` 在 Android 下调用 `run_sidecar`，sidecar 会：

1. 监听本机随机端口。
2. 用 `--webroot` 托管前端静态文件。
3. 通过 stdout 输出端口和 token。
4. 在 `index.html` 的 `<head>` 中注入 `window.__CCS_SIDECAR__`。

WebView、RPC 和 SSE 都走同一个本地服务。认证使用 `x-ccs-token`，不是 `Authorization: Bearer`。

## 前端侧

Android 构建只启用 `src/android-bridge/` 的 Tauri API 替身；React 页面结构、Tailwind 样式和交互保持上游 CCS 原样。针对手机 WebView 的安全区、窄屏布局、触控目标、图标收缩和新版顶栏结构，Android 产物额外注入 `src/android-bridge/mobile.css` 移动适配层；桌面构建不会注入该文件：

Android Web 产物由 `android-frontend-build.yml` 在 GitHub Actions 生成并上传为 `ccs-android-dist` Artifact。`package.json` 中的 `build:android` 只是 CI 使用的脚本名；本地不把它作为发布构建入口。桌面开发仍使用 `pnpm build:renderer`，与 Android 资产流程分开。

Android alias 只在 `CCS_TARGET=android` 时生效，桌面构建仍使用原来的 Tauri API。

## CI 工作流

| 工作流 | 用途 |
| --- | --- |
| `android-shim-probe.yml` | 编译和测试 Android shim |
| `android-sidecar-build.yml` | 交叉编译 `aarch64-linux-android` sidecar |
| `android-frontend-build.yml` | 构建 Android WebView 前端 |
| `android-proxy-tests.yml` | 在宿主 target 跑代理层单测 |

sidecar 构建会检查 Android 依赖图，避免桌面 Tauri、OpenSSL 和 aws-lc 相关依赖混入错误的 target。

## 产物和 ZeroTermux 集成

sidecar 和前端分别发布为：

| 文件 | 用途 |
| --- | --- |
| `libccsidecar.so` | Android aarch64 原生 sidecar |
| `ccs-web.zip` | WebView 前端静态文件 |

当前记录的资产基线：

- CCS Android 资产：`ccs-android-test-a7ae68dbc238`
- ZeroTermux APK：`ccs-2f8353d`
- 上游基线、分支头和最新状态：接手时以 Git 实际结果为准，不从本段旧记录猜测

ZeroTermux 在 `app/build.gradle` 中锁定 tag、SHA-256 和文件大小。更新流程是：

1. 修改 `android-sidecar` 并跑测试。
2. 构建 sidecar 和前端，发布新 `ccs-android-<commit>`。
3. 更新 ZeroTermux 的 `ccsArtifactTag`、SHA-256 和 size。
4. 构建 APK，再检查 APK 内的 sidecar 和前端是否与发布资产一致。

Android 安装包内的 `.so` 和前端资源不能在运行时替换，所以更新 CCS 后需要重新构建 APK。

## 同步上游

```bash
git fetch origin --prune
git merge-base origin/main android-sidecar
git diff --stat $(git merge-base origin/main android-sidecar)..android-sidecar
```

同步后至少运行：

```bash
cd src-tauri
cargo fmt --check
cargo check --lib
cargo test --lib
cargo test --manifest-path android/Cargo.toml
```

Android app data 目录不允许创建 hard link。Codex auth 的安全恢复在 Android 使用
`renameat2(RENAME_NOREPLACE)`，避免恢复旧认证时覆盖并发登录产生的新文件。

## 当前验证

最近一次同步的本机结果：

- `cargo check --lib`：通过
- `cargo test --lib`：2654 通过，0 失败，5 忽略
- `services::proxy::tests`：86/86 通过
- Android shim：39/39 通过
- Chat→Responses：27/27 通过
- Anthropic→Responses：24/24 通过
- ZeroTermux arm64 Debug/Release APK：构建成功

## 相关路径

- `src-tauri/android/`：Android shim workspace
- `src-tauri/src/sidecar.rs`：sidecar HTTP、RPC 和 SSE
- `src/android-bridge/`：前端 Android 替身
- `ZeroTermux/app/build.gradle`：APK 使用的 CCS 产物版本和校验值


## Termux-CCS 固定流程

本 README 只说明 Android sidecar 的技术实现。资产构建、CI、Artifact 下载、ZIP 根目录校验、sidecar 复用、Release 双资产上传、ZeroTermux `app/build.gradle` 锁定、APK 云端构建和手机回归，统一执行 Termux-CCS 知识库的 [唯一构建、测试与发布流程](../../codex/Termux/docs/build-and-release.md) 的 S0-S10。

固定要求：

- `android-frontend-build.yml` 负责 Android Web Artifact；
- `android-shim-probe.yml`、`android-sidecar-build.yml`、`android-proxy-tests.yml` 负责 shim、sidecar 和代理测试；
- 同一 CCS 资产 tag 必须同时提供 `ccs-web.zip` 与 `libccsidecar.so`；
- ZeroTermux 只消费已发布且哈希通过的资产，再由自己的 `build.yml` 云端构建 APK；
- 手机只安装经过文件级校验的 APK，不直接以独立 `ccs2` 作为最终入口；
- 提交、推送、Release、安装和删除都按 SOP 的确认闸门处理。
