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
      已验证：宿主机 + `aarch64-linux-android` 双通过

### 待做

- [ ] **摘除 5 个桌面 crate**（共 49,914 行）
      `runtime-host-desktop` 23,795 / `host-desktop` 11,592 / `overlay-runtime` 7,554 /
      `vr-overlay-backend` 4,623 / `vr-overlay` 2,350
      `composition` **不用改**（`RuntimeHostDesktopAssemblyDeps` 只是中性类型组合，历史误名）
- [ ] **`RuntimeHostProfile::Mobile`** 变体（`crates/application-core`），composition 里 8 处 match
- [ ] **新增 `runtime-host-mobile`**（预计 2–4k 行，对照 `host-desktop` 11.6k + `runtime-host-desktop` 23.8k）
      实现：app data dir、sqlite 路径、本地通知、深链、能力位、无日志源
- [ ] **`src-tauri` 平台分层** —— 159 个文件目前 `cfg(desktop)`/`cfg(mobile)` 各 0 处；
      对两个桌面 crate 有 ~145 处引用散在 35 个模块，需逐个判定
- [ ] **`default_app_data_dir()` 换实现** —— `dirs::config_dir()` 在 Android 返回 None
      （`dirs-sys` 对 android/ios 的 `fallback()` 直接 `None`），改用 Tauri `app.path().app_data_dir()`
- [ ] **TLS 根证书（运行时阻塞，编译不受影响）**
      `tokio-tungstenite` 用的是 `rustls-tls-native-roots`；Android 上
      `rustls-native-certs` 走 unix 分支读 `/etc/ssl/certs`（不存在）→ 连不上 VRChat 的 WS
      → 改 `rustls-tls-webpki-roots`（内置 Mozilla 根证书）
      注：`reqwest` 那条路走的是 `rustls-platform-verifier`（支持 Android，用系统验证器），
      但要确认是否需要 JNI 初始化
- [ ] **（可选）去掉 aws-lc-rs**
      实测能在 Linux 上编过 Android，**不是阻塞**。留在图里的代价只是构建慢 + 要 cmake + 体积。
      真要减，就把 workspace 的 `reqwest` 改成 `default-features = false` 只开需要的特性
- [ ] **`secrets_key` 真实现** —— Android Keystore / iOS Keychain 生成随机 32 字节 key
      目前是 `None` 桩，**不能就这样发布**（cookie / PG 密码会明文落盘）
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
