# CC Switch Android 集成

本项目将 CC Switch 适配到 [ZeroTermux-CCS](https://github.com/AGAGAG666/ZeroTermux-CCS)，提供供应商、模型、Skills 和配置管理，以及本地协议路由。支持在应用内或浏览器访问，并针对手机窄屏和触控操作调整布局。

基于 [farion1231/cc-switch](https://github.com/farion1231/cc-switch) [v4.0.9](https://github.com/farion1231/cc-switch/releases/tag/v4.0.9)，上游提交 [`8845b1ef`](https://github.com/farion1231/cc-switch/commit/8845b1ef799aa8810b864b122de6af5f4629de10)。本项目为非官方适配，与上游无从属或背书关系；许可证为 MIT。

## 使用

在 ZeroTermux 菜单中打开 CC Switch。浏览器访问默认地址为 `http://127.0.0.1:17132/`，Web 端口可在 ZeroTermux 设置中修改。路由代理端口为 `15721`，与页面访问端口不同。

- 支持 `ccswitch://v1/import` 链接，经确认后导入配置。
- 供应商选择、Skills、授权中心、使用统计、应用管理和设置页适配手机布局。
- 内置页面的 SQL 备份导入可调用 Android 系统文件选择器，选中文件后仍需确认导入。
- SQL 导出尚未接入系统“另存为”；浏览器上传和 ZIP 选择暂无 Android 专用适配。目录选择使用 Termux 文件路径。

安装包见 [ZeroTermux-CCS Releases](https://github.com/AGAGAG666/ZeroTermux-CCS/releases)，供宿主集成的资产见 [CC Switch Android Releases](https://github.com/AGAGAG666/cc-switch/releases)。

## 架构

| 目录 | 职责 |
| --- | --- |
| `src-tauri/` | Rust 核心、配置、模型、协议路由和 sidecar |
| `src-tauri/android/` | Android 使用的 Tauri runtime、插件和宏替代实现 |
| `src/android-bridge/` | Tauri API 适配、RPC、SSE 和移动端样式 |
| ZeroTermux 仓库 | WebView、系统文件选择、服务生命周期和 APK 封装 |

桌面构建仍使用真实 Tauri API。Android API alias 和移动样式只在 `CCS_TARGET=android` 构建中生效，业务逻辑不在宿主重写。

sidecar 通过 `--webroot` 提供前端，Web、`/rpc/{command}` 和 `/events` 使用同源连接。前端以 `window.__CCS_SIDECAR__` 握手信息为准；RPC 使用 `x-ccs-token`，SSE 使用 token 参数。不要记录真实凭证或含密钥的导入链接。

## 构建

```sh
pnpm install --frozen-lockfile
pnpm typecheck:android
pnpm typecheck
pnpm build:android
```

本地前端构建用于开发验证；交付资产通过 GitHub Actions 生成。

| workflow | 用途／产物 |
| --- | --- |
| `android-frontend-build.yml` | 类型、桥接和布局检查，生成 `ccs-web.zip` |
| `android-sidecar-build.yml` | 编译 arm64 sidecar，供宿主以 `libccsidecar.so` 集成 |
| `android-shim-probe.yml` | 验证 Android Tauri 替代实现 |
| `android-proxy-tests.yml` | 代理与协议测试 |

APK 构建使用同一源码提交生成的 Web 和 sidecar，宿主锁定两份资产的大小与 SHA-256。Web ZIP 根目录须包含 `index.html` 和 `assets/`。
