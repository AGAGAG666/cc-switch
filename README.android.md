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

Android 构建只启用 `src/android-bridge/` 的 Tauri API 替身；React 页面结构、Tailwind 样式和交互保持上游 CCS 原样，不注入额外的移动端 CSS 覆盖层：

```bash
CCS_TARGET=android pnpm build:android  # 输出 dist-android/
pnpm build:renderer                   # 桌面前端
```

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

当前发布版本：

- CC Switch sidecar：`ccs-android-2f8353d`
- ZeroTermux APK：`ccs-2f8353d`
- 上游基线：CC Switch `v3.20.0`，`origin/main = 0b5da510`
- 当前分支头：`d20ce243`

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
