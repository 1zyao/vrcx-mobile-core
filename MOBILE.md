# MOBILE — VRCX-0 手机端分支

这个分支是 `D:\vrcx0\src`（桌面主线，`sync-main`）的**独立副本**，只做手机端。

- 来源：`D:\vrcx0\src` @ `9e9fe29ac`
- 分支：`mobile`
- 目标：**手机作为完整本地 VRCX-0 客户端** —— 自带 SQLite、作为一台设备加入 remoteSync

桌面主线不受影响，两边各自演进。同步上游改动的做法见文末。

---

## 边界：手机端放弃什么

VRChat 的日志 / Steam / 注册表在手机上不存在，所以这些能力在手机端**不可用**（`HostCapabilities` 里置 `supported: false`，UI 按能力位隐藏）：

```
game_log_watcher            runtime_game_log_ingest
runtime_game_log_side_effects  runtime_game_client_lifecycle
game_process_monitor        vrchat_path_discovery
steam_library_discovery     steam_runtime_integration
registry_prefs              game_launch
vrchat_launch_pipe          screenshot_cache（Android 有限、iOS 基本不行）
```

可用的：本地 SQLite、VRChat API、实时 WS、feed、好友、收藏、通知、活动图表、remoteSync。

**注意**：日志派生事件只在桌面端跑着 VRChat 时才有，那时桌面端进程必然存活、优先级更高，会自动成为采集者。手机设成 `collector=auto, priority=10` 即可（**别填 20**，与桌面平手会按 device_id 掷硬币）。

---

## 工作队列（M1：Rust 上安卓）

### 已完成

- [x] `machine-uid` 按 target 门控（`3f558c8a1`）—— 桌面派生逻辑不变，老库继续可解
- [x] `arboard` / `trash` 按 target 门控（`b2484f60f`）—— 两者都不支持 Android 且**编不过**
      （`trash` 缺 `platform` 模块、`arboard` 缺内部 `Clipboard/Get/Set/Clear`）；
      `move_to_trash` / 两个 clipboard 入口在移动端返回 `unsupported`
- [x] Tauri 桌面 API 逐个门控（`17fb175dc`、`dfdf6b35b`）：tray / menu / window-state /
      autostart / single-instance / skip_taskbar / unminimize / pick_folder，
      builder 链按 `#[cfg] let builder = builder.plugin(...)` 拆开
- [x] `default_app_data_dir()` 支持移动端（`49b1fd1bd`）
      Android → `/data/user/0/<package>/files/<APP_DIR_NAME>`，package 从 `/proc/self/cmdline` 取
      iOS → `$HOME/Library/Application Support/<APP_DIR_NAME>`；桌面分支字节级不变
- [x] VRChat 实时 WS 的根证书 → `rustls-tls-webpki-roots`
      `rustls-native-certs` 把 Android 当普通 unix 去读 `/etc/ssl/certs`（不存在），
      握手直接报 `no native root CA certificates found`。现在用内置 Mozilla 根证书。

**里程碑**：`cargo check --target aarch64-linux-android -p vrcx-0` **通过**，整个应用能编到 Android。

### 待做

- [x] **reqwest 的 HTTPS 根证书** —— 用 `use_preconfigured_tls` 塞 webpki-roots 的 `ClientConfig`
      reqwest 0.13.5 把 `rustls-platform-verifier` 写死了，没有 `rustls-tls-webpki-roots` 这个 feature；
      而 platform-verifier 在 Android 上必须先 `init_with_env` / `init_with_runtime`
      （要 JNI `Env` + Android `Context`），否则证书校验直接失败。
      选了纯 Rust 的方案：在 `vrchat-client/src/web_client.rs` 唯一的 `Client::builder()` 处
      （`fn ... jar, proxy_url, user_agent, follow_redirects`）按 target 注入
      webpki-roots 的 `rustls::ClientConfig`，和 `remote_sync_postgres.rs` 现有写法一致。
      实际只有一个构造点，`outbound-adapters::web_client` 是转调它，所以 `integrations` 也一起修好了。
- [x] **`secrets_key`**：移动端不再返回 `None`（`9e7da1790` 之后的提交）
      桌面走 `machine_uid` 派生不变；Android/iOS 改为**首跑生成 32 字节随机 key**，
      写到应用私有目录的 `secrets.key`（unix `0600`）。该目录本身由应用 UID 独占，
      其他应用读不到 —— 不再是明文落盘。
- [ ] **（升级项）把 `secrets.key` 换成 Keystore / Keychain**
      这需要 JNI `Env` + Android `Context`（或 iOS ObjC 桥），也就是在
      `tauri android init` 生成的 `MainActivity.kt` 里加一个钩子。当前的文件方案不是硬件级。
- [ ] **出 APK**：`tauri android init` → `npm install && npm run build`（出 `dist/`）→ `tauri android build`
- [ ] **UI 触屏化** —— 390px 下主内容仅 118px，零宽度断点，804 处右键菜单
- [ ] **`tauri.conf.json`** —— `targets: ["nsis"]`、`decorations: false`（自绘标题栏手机上是假标题栏）、
      `trayIcon`、`updater` 都要处理
- [ ] **（可选）去掉 aws-lc-rs** —— 实测能编过 Android，**不是阻塞**，只是构建慢 + 要 cmake + 体积

### 已作废的早期判断

最初计划"摘掉 5 个桌面 crate + 新建 `runtime-host-mobile`"，实测证明**不需要**：
把两个不兼容的 C crate 门控掉之后，`host-desktop` 和 `runtime-host-desktop` 原样就能编过 Android，
`composition` 更是完全没动过。代价是移动端多带了一些无用代码，好处是改动面小、和上游结构一致。
- [ ] **`game_log_watcher` 加 feature 关闭**（低优先）
      `start()` 只在 `cfg(windows)`、`start_without_process_monitor()` 只在 `cfg(linux)`。
      实测 Android 上**不报错**，只是这些函数变成 dead code（11 条 warning）。
      加 feature 只是为了让 Android 构建干净、并明确表达"日志源不存在"
- [ ] **`tauri.conf.json`** —— `targets: ["nsis"]`、`decorations: false`（自绘标题栏手机上是假标题栏）、
      `trayIcon`、`updater` 都要处理

---

## 本机交叉编译配方

```powershell
$ndk = 'C:\Android\Sdk\ndk\27.3.13750724'   # r27d
$env:PATH = 'D:\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.51.36231\bin\Hostx64\x64;' + $env:PATH
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = 'link.exe'   # 覆盖仓库 .cargo/config.toml 的 lld-link
$env:ANDROID_NDK_HOME = $ndk
$env:NDK_HOME = $ndk
$env:CC_aarch64_linux_android  = "$ndk\toolchains\llvm\prebuilt\windows-x86_64\bin\aarch64-linux-android24-clang.cmd"
$env:AR_aarch64_linux_android  = "$ndk\toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-ar.exe"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$ndk\toolchains\llvm\prebuilt\windows-x86_64\bin\aarch64-linux-android24-clang.cmd"

cargo check --target aarch64-linux-android -p <crate>
```

- 用 API **24** 因为 Tauri 的最低支持版本是 Android 7.0 (SDK 24)
- `cargo check` 不需要 NDK 链接器，但 build script / proc-macro 要宿主机链接器；编 C 库要 `CC_*` / `AR_*`
- 输出重定向要用 `cmd /c "cargo ... > file 2>&1"`（PowerShell 里 `2>&1 |` 会被拒）

### 已知本机问题

`aws-lc-sys` / `ring` 的 **`.S` 汇编**在本机失败：
`clang: error: unable to make temporary file: Permission denied`

- 只影响 `.S`；`.c`（sqlite3.c / zstd）完全正常
- `-j 1` 仍失败 ⇒ 不是并发争用；手工用同样命令编同一个 `.S` 反而成功
- ⇒ 本机环境特有，不是项目问题。可在 WSL / Linux / macOS CI 上验证

### 已验证能过的（WSL Ubuntu + NDK r27d，`--target aarch64-linux-android`）

```
vrcx-0-composition 及其全部依赖    ✅ 1m08s，0 warning
  ↳ application / application-core / application-sync / application-activity
    application-realtime / application-game
  ↳ persistence 46,616 行        ← libsqlite3-sys(sqlite3.c) + zstd-sys
  ↳ media                        ← libwebp-sys
  ↳ outbound-adapters            ← rusqlite / tokio-postgres / rustls / ring
  ↳ integrations, vrchat-client  ← reqwest / hyper-rustls / rustls-platform-verifier
                                   tokio-tungstenite / aws-lc-sys
platform / contracts / core / vrchat-protocol / runtime-event / i18n   ✅
integration-api / assistant / mcp / logger-protocol                   ✅
```

5 个 C 库全部编过：`aws-lc-sys`、`libsqlite3-sys`、`libwebp-sys`、`ring`、`zstd-sys`。

**结论：中性核心（约 215k 行）100% 能编到 Android。**

### 唯一编不过的地方

```
cargo check --target aarch64-linux-android -p vrcx-0
  error[E0433]: cannot find module or crate `platform` in this scope
  error[E0599]: no method named `delete_all_canonicalized` found for `&TrashContext`
  error: could not compile `trash` (lib)
```

`src-tauri` → `host-desktop` → **`trash`** 不支持 Android。这就是那道桌面边界，
正好印证了「摘掉 5 个桌面 crate」是 M1 的主要工作。

---

## 与上游同步

```powershell
cd D:\vrcx0\mobile
git fetch D:\vrcx0\src sync-main
git merge FETCH_HEAD        # 或 git rebase
```

因为两边历史同源（`mobile` 从 `9e9fe29ac` 分出），所以直接 merge 即可，不需要打补丁。
建议桌面主线每稳定一批就同步一次，不要让分叉拉太大。

## 相关文档

- `D:\vrcx0\mobile-android-audit.md` —— 完整静态审计 + 实测结果
- `D:\vrcx0\README.md` —— 整个工作区的总览

---

## 出 APK 的方法（本机实测通过）

产物：`D:\vrcx0\out\android\VRCX-0-mobile-debug-arm64.apk`（debug 338 MB）

### 三处必须加的环境/工程调整

1. **`CFLAGS_aarch64_linux_android=-save-temps=obj`**
   本机 `clang` 在编译 `.S` 汇编时 `unable to make temporary file: Permission denied`
   （`.c` 正常）。让它把中间文件写到目标对象旁即可绕开。

2. **`DOCS_RS=1`**
   `tauri-plugin` 的 build script 要在 cargo registry 里给每个插件建
   `<crate>/android/.tauri/tauri-api`，本机 cargo 子进程没权限建目录（os error 5）。
   `tauri-plugin/src/build/mobile.rs:114` 有 `DOCS_RS` 逃生门，直接跳过这段。
   **代价**：插件的 Android 端工程不会被 Gradle 链接进去（fs / notification / deep-link
   这些插件在手机上可能不工作）。要根治得让这些目录可写，或改用可写的 vendor 目录。

3. **`compileSdk`/`targetSdk` 改成 36**（`src-tauri/gen/android/app/build.gradle.kts`）
   模板要 37，本机只装了 platform 36。`gen/` 是 gitignore 的，重新 `tauri android init` 后要再改一次。

另外 Gradle 要走代理：`src-tauri/gen/android/gradle.properties` 里加了
`systemProp.http(s).proxyHost/Port`。

### 工程侧的三处改动（已提交，不是临时 hack）

- `src-tauri/Cargo.toml` 加 `[lib] crate-type = ["staticlib","cdylib","rlib"]`
  —— 移动端要 `libvrcx_0.so`，没有 cdylib 出不来
- `src-tauri/src/lib.rs` 用 `#[cfg_attr(mobile, tauri::mobile_entry_point)] pub fn run()`
  —— 否则 `tauri android build` 报 "does not include required runtime symbols"
- `src-tauri/tauri.android.conf.json` 把 `beforeBuildCommand` 置空
  —— Gradle 里起不动 `npm.bat`；前端产物已由 `npm run build` 单独生成到 `dist/`

### 完整命令

```powershell
cd D:\vrcx0\mobile
npm install; npm run build            # 出 dist/

$ndk = 'C:\Android\Sdk\ndk\27.3.13750724'
$env:ANDROID_HOME='C:\Android\Sdk'; $env:ANDROID_SDK_ROOT='C:\Android\Sdk'
$env:NDK_HOME=$ndk; $env:ANDROID_NDK_HOME=$ndk
$env:JAVA_HOME='C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot'
$env:PATH = 'D:\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.51.36231\bin\Hostx64\x64;' + $env:PATH
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = 'link.exe'   # 覆盖 .cargo/config.toml 的 lld-link
$env:CC_aarch64_linux_android  = "$ndk\toolchains\llvm\prebuilt\windows-x86_64\bin\aarch64-linux-android24-clang.cmd"
$env:AR_aarch64_linux_android  = "$ndk\toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-ar.exe"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $env:CC_aarch64_linux_android
$env:CFLAGS_aarch64_linux_android = '-save-temps=obj'
$env:DOCS_RS = '1'
$env:HTTP_PROXY='http://127.0.0.1:7890'; $env:HTTPS_PROXY='http://127.0.0.1:7890'

npx tauri android build --debug --apk --target aarch64
```

要 x86_64（模拟器）就把 `aarch64` 换成 `x86_64`，同时把 `CC_`/`AR_`/`LINKER`
换成 `x86_64-linux-android24-clang`。

### 还没做的

- **release 签名**：现在只有 debug APK。release 要配 `signingConfig` + keystore。
- **真机/模拟器验证**：还没有设备跑过，所以前面三个运行时修复仍只是编译级验证。

---

## 运行时验证（2026-11-16，MuMu 12 / Android 15 / SDK 35 / x86_64）

安装 `VRCX-0-mobile-debug-x86_64.apk` 后启动，**登录界面正常渲染**，
进程存活无 `FATAL` / `AndroidRuntime` / `ClassNotFoundException`。

三个运行时阻塞的实测证据：

| # | 修复 | 实测证据 |
|---|---|---|
① | app data dir | `/data/user/0/com.vrcx_0.app/files/VRCX-0/` 下生成 `VRCX-0.sqlite3` (282 KB) + `-wal`/`-shm` |
③ | secrets key | `secrets.key` **32 字节**，权限 `-rw-------` |
② | TLS webpki roots | **只有编译级验证** —— 需要真实 VRChat 账号登录才能触发 HTTPS/WS，未验 |

## 关键前提：依赖必须 vendored

本机 `D:\vrcx0` 被打了 **低完整性标签**：

```
D:\vrcx0          Mandatory Label\Low Mandatory Level:(OI)(CI)(NW)
D:\Tools\...\registry       ← 无标签（默认 Medium）
```

于是 cargo 拉起的 build script / clang 只能写工作区内的目录，写 registry 会
`os error 5 (拒绝访问)`。表现就是：

- `tauri-plugin` 无法在 registry 里建 `android/.tauri` → 插件 Kotlin 类进不了 APK
  → 启动时 `WryActivity.getAppClass` 抛 `ClassNotFoundException` 直接崩
- `clang` 编译 `.S` 时 `unable to make temporary file`（系统临时目录在 Medium）

解法：`cargo vendor --versioned-dirs vendor`（1.19 GB，已 gitignore）+
`.cargo/config.toml` 里 source replacement。依赖落在工作区内，写入就正常了。
`vendor/` 缺失时重新生成即可。

> 如果不想占 1.2 GB，替代方案是把 `CARGO_HOME` 指到工作区里
> （`D:\vrcx0\.cargo-home`），代价是依赖重新下载一遍。
