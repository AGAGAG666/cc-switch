# android-sidecar 分支说明

本分支为 [ZeroTermux-CCS](https://github.com/AGAGAG666/ZeroTermux-CCS) 提供
Android aarch64 的 cc-switch 运行时产物，使 cc-switch 能在手机上以
「原生 sidecar + WebView 前端」形态运行，不依赖 Tauri、不依赖桌面图形栈。

**非官方分支。** 与 cc-switch 上游作者无关联，未获背书。
上游为 [farion1231/cc-switch](https://github.com/farion1231/cc-switch)，
`LICENSE`（MIT © 2025 Jason Young）逐字节保留未改。

## 设计原则：上游业务逻辑少改、兼容层集中维护

cc-switch 本体与前端**不做移植式重写**，主要通过替换依赖层完成 Android 适配。
少量必须跟随上游 API 变化的兼容点集中在 `cfg(target_os = "android")`、sidecar
入口和统一命令表中，避免把 Android 分支改成脱离上游的独立实现。

| 层 | 手段 |
|---|---|
| Rust 后端 | 用 9 个 shim crate 顶替 `tauri` 及其插件，`Cargo.toml` 按 `cfg` 换 path 依赖 |
| React 前端 | vite alias 把 `@tauri-apps/*` 指向 `src/android-bridge/*` |

## Rust 侧：src-tauri/android/

独立 workspace（不干扰 `src-tauri` 本体的依赖解析），9 个 crate：

| crate | 作用 |
|---|---|
| `tauri-shim` | `tauri` 本体替身。拆为 `app/plugins/rpc/runtime/state/window` 六个模块 |
| `tauri-shim-macros` | 复刻 `#[command]` 与 `generate_handler!`，让 v3.20.0 上游 **303 个命令**原样编译 |
| `arboard-shim` | 剪贴板。桌面走 X11/Win32/AppKit，Android 无此栈，改走 `termux-clipboard-set` |
| `auto-launch-shim` | 开机自启。Android 无对应语义，实现为安全空操作 |
| `tauri-plugin-updater-shim` | 自动更新。移动端由宿主 APK 负责，此处置空 |
| `tauri-plugin-dialog` | 对话框，转发到 `tauri-shim::plugins` |
| `tauri-plugin-opener` | 外部打开，同上 |
| `tauri-plugin-store` | 键值存储，同上 |
| `shim-selftest` | 用**上游真实命令签名**验证宏展开正确性（659 行） |

`shim-selftest` 是这套 shim 的正确性保障。它覆盖 camelCase 参数改名、
`State<'_, T>` 多状态、`AppHandle<R>` 泛型、`Window`、非 Result 返回、
自定义 Display 错误、`Option<T>` 缺键、`Emitter`、三个插件 Ext trait ——
每一项都对照过 `src-tauri/src/` 里的实际写法。

### sidecar 入口

`src-tauri/src/main.rs` 在 Android 下走 `run_sidecar(port, webroot)`，
以 `--webroot <DIR>` 同源托管前端。webroot 取值优先级：

```
--webroot <DIR>  >  --webroot=<DIR>  >  CCS_SIDECAR_WEBROOT  >  无
```

启动后向 stdout 打印握手行交出 `port` 与 `token`，并在 `index.html` 的
`<head>` 注入 `window.__CCS_SIDECAR__`，前端 shim 同步命中。

## 前端侧：src/android-bridge/

12 个模块顶替 9 个 `@tauri-apps/*` 包，另含 `mobile.css` 移动端覆盖层。
构建开关是环境变量：

```bash
CCS_TARGET=android npm run build    # 产物 dist-android/，base "./"
npm run build                       # 桌面产物 dist/
```

`vite.config.ts:14` 判定 `isAndroid`，仅在该模式下注入 alias 与 CSS，
**桌面构建路径完全不受影响**。

## CI 工作流

四个，均可 `workflow_dispatch` 手动触发：

| 工作流 | 作用 |
|---|---|
| `android-shim-probe.yml` | 只编 `src-tauri/android` 子 workspace。先在宿主 target 清类型错误，再验 aarch64 交叉编译 |
| `android-sidecar-build.yml` | 交叉编译 163k 行**本体**到 `aarch64-linux-android`，暴露 shim 接缝上的 cfg 遗漏 |
| `android-frontend-build.yml` | 出 Android 前端静态产物 |
| `android-proxy-tests.yml` | 跑本体单元测试（本分支原本没有这个入口） |

前两个的分工是刻意的：shim-probe 快、只管替身自身编得过；sidecar-build 慢、
验证替身**真的能顶住上游全部调用点**。

## 产物与消费方式

`android-sidecar-build.yml` 与 `android-frontend-build.yml` 的产物发布为
Release，tag 命名 `ccs-android-<commit 短 sha>`：

| 产物 | 内容 |
|---|---|
| `libccsidecar.so` | aarch64 原生 sidecar |
| `ccs-web.zip` | React 前端静态产物 |

ZeroTermux-CCS 侧在 `app/build.gradle` 用 `ext.ccsArtifactTag` 钉住 tag，
下载后校验 SHA-256 与字节数，任一不符即构建失败。

**升级 CCS 版本的步骤**（宿主 APK 代码无需改动）：

1. 本分支提交改动，触发上述工作流，发布新 tag `ccs-android-<新短sha>`
2. ZeroTermux-CCS 改 `app/build.gradle` 的 `ext.ccsArtifactTag` 及对应
   `sha256` / `size`
3. 重新构建 APK

已装在设备上的 APK 无法在运行时替换 sidecar（原生 `.so` + assets 内前端），
必须重新构建安装。

## 上游同步状态

- 当前上游基线：CC Switch `v3.20.0`，`origin/main = 0b5da510`。
- 当前 Android 分支头：`daa3a2ae`；相对上游保留 34 个 Android/代理定制提交。
- 同步前回滚分支：`backup/android-sidecar-before-v3.20.0-20260821-222458`。
- 本次同步补齐了 v3.20.0 新增的 Pi prompt/session 和 OpenCode model 命令；
  桌面与 sidecar 的统一命令表现为 303 条命令。
- v3.20.0 将 `CodexOAuthState` 改为直接持有 `Arc<CodexOAuthManager>`；
  sidecar 初始化已同步调整，并恢复桌面 deep-link 函数的 Android 条件编译。

### 本次验证

- `cargo fmt --check`：通过。
- `cargo check --lib`：通过。
- Chat→Responses streaming：27/27 通过。
- Anthropic→Responses streaming：24/24 通过。
- Android shim workspace：39/39 通过；3 个 doc test 按设计忽略。
- 全量 `cargo test --lib`：2654 通过、0 失败、5 忽略。
- `services::proxy::tests`：86/86 通过，覆盖 Codex auth 恢复、回滚及并发登录保护。
- Android app data 目录受 SELinux 限制，不能创建 hard link；Codex auth 安全恢复
  在 Android 改用内核 `renameat2(RENAME_NOREPLACE)`，目标已被新登录创建时原子
  返回 `EEXIST`，不会覆盖更新的官方认证。桌面仍沿用上游 hard-link 事务。

> 源码同步不等于手机 APK 已升级。ZeroTermux 当前仍锁定
> `ccsArtifactTag = ccs-android-6bec653`；生成新 sidecar/前端产物、更新 tag、
> SHA-256 与 size，再重建 APK 后，设备中的 CCS 才会切换到 v3.20.0。

## 与上游同步

分支点见 `git merge-base origin/main android-sidecar`。
比较改动请以分支点为基准，否则上游后续提交会被误读成本分支的删除：

```bash
git diff --stat $(git merge-base origin/main android-sidecar)..android-sidecar
```

因改动集中在新增目录、`cfg` 分支和统一命令表，rebase 上游时通常只需处理
Tauri bootstrap、命令注册表及上游接口类型变化。每次同步后必须重新执行 shim、
proxy 和 Android 构建验证。
