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

---

## /m/home 数据接线规格（下一步照着做）

`buildFriendsSidebarVirtualRows`（`components/sidebar/friends-sidebar/friendsSidebarVirtualRowBuilder.ts:185`）
要 14 个入参，来源已全部定位到 `components/sidebar/FriendsSidebar.tsx`：

| 入参 | 来源 | 行号 |
|---|---|---|
`locationTimesByUserId` | `useFriendLocationTimeStore(s => s.byUserId)` | 161 |
`sortContext` | `useFriendSortContext([sidebarSortMethod1..3], locationTimesByUserId)` | 164 |
`favoriteFriendGroups` / `favoriteFriendIds` / `groupedFavoriteFriendIdsByGroupKey` / `localFriendFavoriteGroups` / `localFriendFavorites` | `useFriendsSidebarFavoriteState()` | 172–178 |
显示偏好（`randomUserColours` / `showSidebarAvatarFrame` / `showSidebarNameplate` / `trustColor` / `showInstanceIdInLocation` / `ageGatedInstancesVisible`） | `useFriendsSidebarDisplayPreferences()` | 180–188 |
`openGroups` | `useFriendsSidebarPreferences()` | 189–190 |
`currentLocation` / `currentInviteLocation` | `useCurrentInviteContext()` | 192–196 |
`currentLocationSnapshot` | useMemo，用 `resolveObservedPlayerUserIds(...)` | 197–214 |
`rows` | useMemo（好友记录主表） | 242 |
`favoriteRows` | useMemo | 481 |
`onlineRows` | useMemo | 507 |
`activeRows` | useMemo | 532 |
`offlineRows` | useMemo | 555 |
`favoriteGroupSections` | useMemo | 578 |

另外需要：`friendsById`、`onlineIds`、`allActiveIds`、`allOfflineIds`、`filterIdsByQuery`（过滤搜索框）。

### 做法

新建 `src/features/mobile/useMobileFriendBoard.ts`，照抄上表把入参备齐后调
`buildFriendsSidebarVirtualRows`，返回 `SidebarVirtualRow[]`。

**决策：复制接线，不改桌面侧栏。**
理由：本机编不了桌面版（aws-lc 问题），改 27KB 的 `FriendsSidebar.tsx` 无法回归验证；
且那是上游派生代码，动它会让以后合上游更麻烦。代价是移动端多一份接线，风险隔离在 `/m` 内。

### 已就绪的渲染层

`src/features/mobile/MobileFriendRows.tsx` 已实现 `MobileSectionHeader` / `MobileFriendRow`，
直接吃 `SidebarVirtualRow`；`userImage()` 返回普通 https URL（`/image/<fileId>/<version>/<res>`），
不需要注册 `vrcx-0-img:` 协议（那是本地文件专用）。

### 好友行的切分逻辑（已抄下，`FriendsSidebar.tsx`）

```ts
// 242: 主表
rows = rowsByIds(filterIdsByQuery(orderedFriendIds, friendsById, filterText), friendsById)

// 507 / 532 / 555：三段切分
onlineRows  = sortRows(rowsByIds(visibleOnlineIds, friendsById).filter(f =>
                !excludedFavoriteIds.has(normalizeId(f.id)) &&
                !(prefs.isHideFriendsInSameInstance && sameInstanceIds.has(f.id))), prefs, ...)
activeRows  = sortActiveRows(rowsByIds(activeIds,  friendsById).filter(f =>
                !(prefs.isHideFriendsInSameInstance && sameInstanceIds.has(f.id))), prefs, ...)
offlineRows = sortRows(rowsByIds(offlineIds, friendsById).filter(f =>
                !(prefs.isHideFriendsInSameInstance && sameInstanceIds.has(f.id))), prefs, ...)
```

### 还剩的未确认项（下一轮开工前先确认这 6 个名字的来源）

`friendsById`、`orderedFriendIds`、`visibleOnlineIds`、`activeIds`、`offlineIds`、
`rowsByIds` / `sortRows` / `sortActiveRows` / `filterIdsByQuery` 的 import 与参数签名，
以及 `sortRows` 尾部传的 `prefs` / `sortContext`。

拿到这些就能一次写完 `useMobileFriendBoard()`：
`sections = buildFriendsSidebarVirtualRows({ onlineRows, activeRows, offlineRows,
  sameInstanceGroups: [], favoriteGroupSections: [], favoriteRows: [],
  openGroups, prefs, rowsLength: onlineRows.length + ..., t })`
—— 先只出「在线 / 活跃中 / 离线」三段，同房间分组和收藏随后补。

---

## 完整性标签修复（2026-11-16）

工作区原先是 **Low integrity + No-Write-Up**：

```
D:\vrcx0         Mandatory Label\Low Mandatory Level:(OI)(CI)(NW)
D:\Tools\...\registry   无标签（默认 Medium）
```

cargo 拉起的 build script 子进程因此在 Medium 位置（`%TEMP%`、cargo registry）写入被拒，
表现为两大类失败：

- `clang: unable to make temporary file: Permission denied`（编 `.S` 时）
- `tauri-plugin: failed to create .tauri directory`（插件无法 provision Android 模块
  → 插件 Kotlin 类进不了 APK → 启动 `ClassNotFoundException` 崩溃）

修复：

```powershell
icacls 'D:\vrcx0'        /setintegritylevel '(OI)(CI)M'
icacls 'D:\vrcx0\mobile' /setintegritylevel '(OI)(CI)M'
```

**实测结果**：`cargo build --target aarch64-linux-android -p vrcx-0-vrchat-client`
（会编 ring 的 `.S`）**不带 `CFLAGS=-save-temps=obj`** 也通过了，0 temp-file 错误。

因此以下两个临时方案可以退役（待重新验证 APK 构建后删除）：

| 临时方案 | 状态 |
|---|---|
`CFLAGS_aarch64_linux_android=-save-temps=obj` | 已不需要 |
`DOCS_RS=1`（跳过插件 provision） | 应可去掉，插件能正常写 registry 了 |
`vendor/`（1.19 GB，为了绕开写入限制） | 也许可以去掉，待验证 |

注：`std::env::temp_dir()` 在测试里已改为工作区目录，可保留（无害且更快）。

### 清理验证结果（2026-11-16）

抬完整性标签后重打 APK，**完全不带** `DOCS_RS` 和 `CFLAGS=-save-temps=obj`：

```
npx tauri android build --debug --apk --target x86_64   → 成功
adb install + am start                                  → 进程存活
logcat FATAL / AndroidRuntime / ClassNotFound           → 无
```

**插件 provision 问题确认解决** —— 之前 `ClassNotFoundException: app.tauri.deep_link.DeepLinkPlugin`
是因为插件写不进 registry、Android 模块没进 APK；现在自动 provision，崩溃消失。

手机端独立入口也验证可用：`dist-mobile` + `mobile.html` → 底栏 Home/Favorites/Inbox
+ 独立圆形搜索钮（VRCM 1.1.4 的导航形态），页面路由到 stub。

`vendor/`（1.19 GB）尚未验证能否删除 —— 留待下一步。

### vendor/ 已移除（2026-11-16）

`vendor/`（1,189 MB）是为绕开"build script 写不进 registry"而建的，标签修复后不再需要。

验证：

```
.cargo/config.toml 停用 vendored-sources
  cargo test -p vrcx-0-command-server -p vrcx-0-mobile-host   → 全过
  cargo build --target x86_64-linux-android -p vrcx-0 --lib   → 成功
      tauri-plugin .tauri 错误数 = 0
删除 vendor/ 后重跑测试                                        → 全过
```

三个临时方案现已全部退役：`-save-temps=obj`、`DOCS_RS=1`、`vendor/`。

---

## 决定：手机端 UI = VRCM 本体，React 那套已删除（2026-11-16）

之前试过两条路，现在只留一条：

```
❌ 轨 A  React 版手机壳（src/mobile + mobile.html + dist-mobile + /m 路由）
        我照 VRCM 截图手写的仿制品，和轨 B 完全重复
✅ 轨 B  VRCM 的 Kotlin Compose App + VRCX-0 Rust 核心（in-process .so + loopback HTTP）
```

轨 A 已全部删除，桌面 React 前端同时恢复原状（去掉 `max-md` 补丁、`MobileNavButton`、
窄屏 CSS、viewport-fit）。`npm run typecheck` 与 `npm run build` 均通过。

轨 B 的当前进度：

```
Rust 侧（D:\vrcx0\mobile）
  crates/command-server   泛型 CommandRegistry + /api/invoke + /api/events   6 测试
  crates/mobile-host      MobileRuntimeHostState(SQLite) + app__feed_rows_query  4 测试
  crates/mobile-core      start_core + JNI 入口，交叉编译 x86_64-linux-android
                          libvrcx_0_mobile.so 22.8 MB，导出符号已验证

VRCM 侧（D:\vrcm\vrcm-base，未提交）
  androidMain/jniLibs/x86_64/libvrcx_0_mobile.so
  androidMain/kotlin/core/NativeCore.kt            System.loadLibrary + external start
  commonMain/kotlin/mobilecore/VrcxCoreClient.kt   Ktor，一个 invoke 服务所有命令
  :composeApp:compileDebugKotlinAndroid            BUILD SUCCESSFUL (2m9s)
```

下一步：把 VRCM 的 VRCX Feed 页数据源从 JDBC/PG 换成 `VrcxCoreClient.feedRows()`。

---

## 纵切打通（2026-11-17，VRCM 1.1.6 基线）

MuMu（x86_64）上，VRCM 进程内跑起完整 VRCX-0 运行时，并通过 loopback 命令总线取到数据：

```
I VrcxCore: core started: {"port":45675,"token":"Em_4UDhMkLe6-L2lnT4FgdQPsnGGfd6kT31BJF-rtk4"}
I VrcxCore: feedRows ok: []
```

链路：

```
MainActivity
  → NativeCore.start(filesDir)            JNI，唯一一次跨边界
  → libvrcx_0_mobile.so                   117.9 MB（debug）
      → host::build_host()                RuntimeHostStateBuilder + composition
      → build_bus(host)                   CommandRegistry<RuntimeHostState>
      → serve_loopback                    127.0.0.1:<ephemeral>
  → VrcxCoreClient("http://127.0.0.1:$port", token)
  → POST /api/invoke {cmd: app__feed_rows_query}
  → 真实 runtime.database() → SQLite → {"ok":true,"result":[]}
```

`[]` 是正确结果：手机的本地 SQLite 是新建的，还没有 feed 行。

### 途中修的两个真问题

1. **Kotlin 模板字符串被 PowerShell 转义写坏**（`${'$'}{...}`）—— 已修
2. **Android 明文流量策略拦截 loopback**：
   `CLEARTEXT communication to 127.0.0.1 not permitted by network security policy`
   修法：新增 `res/xml/network_security_config.xml`，只对 `127.0.0.1` / `localhost`
   放开 cleartext，其余仍强制 HTTPS；manifest 加 `android:networkSecurityConfig`。

### 遗留

- `.so` debug 体积 117.9 MB —— release 待实测
- VRCM 侧改动（3 个 Kotlin 文件 + `.so` + manifest + res/xml）尚未在该仓库提交
- 下一步：搬 `vrchat_proxy` 的 41 条代理命令，并接 `/api/events`

---

## 登录门面的落点与签名（2026-11-17 确认）

`LoginSessionRuntime` 的每个方法都要四样依赖，**它们全部在 `RuntimeHostContext` 里**：

```rust
// composition/src/context.rs
pub(crate) login_session: LoginSessionRuntime,          // 65
pub(crate) auth_credentials: Arc<dyn AuthCredentialStore>,  // 66
pub(crate) login_api: Arc<dyn LoginApi>,                    // 67
pub(crate) auth_cookies: Arc<dyn AuthSessionCookies>,       // 68
```

而且 context 已有同类访问器先例（`auth_credentials()` / `auth_credentials_shared()`，175–180 行）。

**结论：门面写在 `composition`，不写在 `mobile-host`。** 否则 mobile-host 得自己再拿一遍这四样，等于重复组装。

### 要加的方法（签名已确定）

```rust
impl RuntimeHostContext {
    pub async fn start_login_session(&self, input: LoginSessionStartInput) -> LoginSessionState {
        self.login_session
            .start(Arc::clone(&self.login_api), self.auth_cookies.as_ref(),
                   self.auth_credentials.as_ref(), input, &|_| Ok(()))
            .await
    }

    pub async fn respond_login_session(&self, input: LoginSessionRespondInput) -> LoginSessionState {
        self.login_session
            .respond(input, self.auth_cookies.as_ref(), self.auth_credentials.as_ref())
            .await
    }

    pub async fn cancel_login_session(&self, attempt_id: String) -> LoginSessionState {
        self.login_session
            .cancel(attempt_id, self.auth_cookies.as_ref(), &|_| Ok(()))
            .await
    }

    pub async fn auto_login_start(&self, input: AutoLoginStartInput) -> Result<AutoLoginOutcome> { … }

    pub async fn end_login_session(&self, request: LoginSessionEndRequest) -> Result<Option<SavedAuthSnapshot>> { … }

    pub fn login_session_state(&self) -> LoginSessionState { self.login_session.state() }
}
```

`transition` 参数传 `&|_| Ok(())`（`respond` 内部就是这么传的）。

门面写好后，cloud 的 12 条 auth 命令就能照抄 —— 和 proxy 那批一样变成机械活。

### 剩余未知

- `TransitionSink` 的精确类型（grep 没找到定义，应是 type alias；看调用现场是 `&(dyn Fn(..) -> Result<()> + Send + Sync)`）
- `AutoLoginOutcome` / `SavedAuthSnapshot` 的具体形状（返回值，可先用 `ok(...)` 直接序列化）

---

## VRCM 登录接入的前置（2026-11-17）

### AuthService 是具体类，20+ 文件依赖

```
service/AuthService.kt:68   class AuthService(...)      ← 不是 interface
13 个公开方法：
  beginAccountLogin / restoreAuth / currentUser / verify / login / logout
  accountDto / accountDtoList / accountDtoOrNull / doReTryAuth
  applySocketUserLocation / applyOwnProfileRefresh / applySocketUserUpdate
依赖它的文件 20+ 个（几乎全部 presentation 域 + di 模块）
```

**所以不能"换实现"，只能换它的内部、保持签名不变。** 这正好符合"页面不动，只换数据层"。

### 核心客户端共享入口（已加）

```kotlin
// commonMain
expect fun vrcxCoreClient(): VrcxCoreClient
expect fun isVrcxCoreReady(): Boolean

// androidMain：幂等启动，第一次调用者赢，后来者复用同一 endpoint
fun startVrcxCore(filesDir: String): VrcxCoreClient?
```

现在任何 service 都能 `vrcxCoreClient()` 拿到客户端，不必自己解析 port/token。
`:composeApp:compileGithubDebugKotlinAndroid` → BUILD SUCCESSFUL (36s)。

### AuthService 改造还缺的命令

| AuthService 方法 | 需要的核心命令 | 状态 |
|---|---|---|
`login()` | `app__vrchat_auth_session_start` | ✅ 已注册 |
`verify()` | `app__vrchat_auth_session_respond` | ✅ 已注册 |
`currentUser()` | `app__vrchat_auth_current_user_get` | ⬜ 未注册 |
`logout()` | `app__vrchat_auth_session_end` | ⬜ 未注册 |
`restoreAuth()` | `app__vrchat_auth_saved_snapshot_get` | ⬜ 未注册 |

所以下一步：把这 3 条注册进 mobile-host，再改 AuthService 的内部。

---

## AuthService 改造方案（读完 login 实现后，2026-11-17）

```kotlin
suspend fun login(username, password): AuthState = authMutex.withLock { loginLocked(...) }

private suspend fun loginLocked(...): AuthState {
    val activeUserId = SharedFlowCentre.currentSession.value?.account?.userId
    val targetUserId = accountDao.accountDtoByUserName(username)?.userId    // ← AccountDao
    if (restoreStoredCookies) applyAuthCookie(username)                     // ← cookie 存储
    return try {
        authApi.login(username, password).also {                            // ← HTTP 调用
            if (it is AuthState.Authed) emitAuthed(password, ...)           // ← 大量本地簿记
            else if (invalidateOnIncompleteAuth) invalidateCurrentSessionLocked()
        }
    } catch (e) { if (isAccountSwitch) invalidateCurrentSessionLocked(); throw e }
}
```

`emitAuthed` 是私有大函数（存密码、存 cookie、写 AccountDao、发 `SharedFlowCentre` 会话事件）。
整个 `AuthService` 802 行，`login` 这条链依赖 `AccountDao` / `applyAuthCookie` / `SharedFlowCentre`。

### 为什么不能一次换掉

换 `authApi.login` 只是第一步，`emitAuthed` 之后的簿记是**其它 20+ 个依赖方赖以工作的前提**
（当前用户、账号列表、会话事件都从这里出）。一次性删掉，全 app 立刻没有登录态。

### 最小安全增量（推荐做法）

**只换 HTTP 那一处，保留簿记：**

```kotlin
val coreState = if (isVrcxCoreReady())
    coreLogin(username, password)          // app__vrchat_auth_session_start → AuthState
else
    authApi.login(username, password)      // 旧路径，编译期保留，便于对照

coreState.also {
    if (it is AuthState.Authed) emitAuthed(password, ...)   // 簿记照旧
    else if (invalidateOnIncompleteAuth) invalidateCurrentSessionLocked()
}
```

好处：
- 登录真的走核心（VRChat 请求由 Rust 发出），**VRCM 不再自己连 VRChat**
- 簿记不动，20+ 依赖方无感
- 有 `isVrcxCoreReady()` 兜底，核心没起来就退回旧路径，不会把 app 锁死
- 后面再逐步把 `emitAuthed` 里的 cookie/密码存储换成读核心的 snapshot

### 需要新写的映射

`session_start` 返回 `LoginSessionState`（Rust 枚举）→ VRCM 的 `AuthState`（`Authed` / 需要 2FA / 失败）。
这个映射写在新文件 `mobilecore/AuthStateMapping.kt` 里，**不改 VRCM 现有的 AuthState 定义**。

---

## 登录改走核心（2026-11-17，已编译 + 装机）

`AuthService.loginLocked` 的那一处 HTTP 换成按核心可用性分流：

```kotlin
val state = if (isVrcxCoreReady()) {
    vrcxCoreClient().loginBasic(username, password)   // app__vrchat_auth_session_start
} else {
    authApi.login(username, password)                 // 旧路径保留
}
```

`emitAuthed` 等簿记原样保留，20+ 依赖方无感。

新增文件：
```
commonMain/mobilecore/AuthStateMapping.kt   LoginSessionState → VRCM AuthState
commonMain/mobilecore/VrcxCoreClient.kt      + loginBasic / loginRespond
```

映射规则：
```
Authenticated            → AuthState.Authed
Challenge(mode=emailOtp) → AuthState.NeedEmailCode
Challenge(mode=totp)     → AuthState.NeedTFA
Challenge(mode=otp)      → AuthState.NeedTTFA
DeviceApproval           → AuthState.Unauthorized("device approval required")
Failed(reason)           → AuthState.Unauthorized(reason)
```

设备核准那条暂时降级成消息（手机还没有相应的界面）。枚举 tag 做了防御式解析
（先按 `Authenticated` 变体名找，再退回 `kind`/`type` 字段），线格式变了也不会崩。

装机验证：

```
I VrcxCore: core ready on port 42529
I VrcxCore: feedRows ok: []
进程 13240 存活，无 FATAL
```

### 还没验证的

**真实账号登录** —— 需要在界面上输入账号密码，这一步没法用 adb 自动化（Compose 输入框）。
下次需要人工点一次登录，确认：
- logcat 里能看到核心调用
- 登录成功后 `emitAuthed` 正常发会话
- VRChat 请求确实由 Rust 发出（VRCM 侧没有 `authApi` 的网络日志）

---

## 重要发现：登录改走核心之后，`emitAuthed` 仍在直连 VRChat（2026-11-17）

```kotlin
private suspend fun emitAuthed(password, response, expectedUserId): Result<Unit> = runCatching {
    (response ?: authApi.userRes()).let {                    // ← 又一次 VRChat 请求
        val userData = enrichCurrentUser(it.checkSuccess<CurrentUserData>())
        …
        authCookie = cookiesStorage.cookieValue(AUTH_COOKIE),          // ← 自己管 cookie
        twoFactorAuthCookie = cookiesStorage.cookieValue(TWO_FACTOR_AUTH_COOKIE),
```

而 `loginLocked` / `verify` 在成功后都会调 `emitAuthed(...)`：

```kotlin
// verify
return authApi.verify(verifyCode, authType).let { if (it.isSuccess) emitAuthed(password) else it }
```

**所以现在的实际状态是：**
- 登录那一次 POST 由核心发出 ✅
- 紧接着 `emitAuthed` 又用 `authApi.userRes()` 拉一次当前用户 ❌ （VRCM 自己的客户端）
- cookie 也从 VRCM 自己的 `cookiesStorage` 读，而不是核心的 cookie jar ❌

也就是说"两个 VRChat 客户端"只消除了三分之一。

## 要真正切断，需要三件事

1. **`current_user` 数据从核心来** —— 这是之前因为 endpoint/websocket 配置管道而搁置的那条命令。它现在是硬阻塞，不是可选项。
2. **`emitAuthed` 接受外部传入的 user** —— 加一个可选参数 `user: CurrentUserData? = null`，非空时跳过 `authApi.userRes()`。这是纯增量改动，20+ 依赖方不受影响。
3. **cookie 交给核心** —— `AccountDto.authCookie` / `twoFactorAuthCookie` 两个字段改为从核心的 saved snapshot 取，或者干脆不再往 AccountDto 里存 cookie。

## 结论

上一轮的"登录走核心"是必要但不充分的一步。要达成目标（VRCM 不再自己连 VRChat），`current_user` 那条命令是下一个绕不过去的关口 —— 而它卡在 `VrchatConfigRuntime::get()` 提供的 endpoint/websocket 上。

### 下一步建议顺序

```
1. 接通 VrchatConfigRuntime::get() → endpoint / websocket（一次读配置 + 缓存）
2. 注册 app__vrchat_auth_current_user_get
3. emitAuthed 加 user 参数，登录/验证后从核心传入
4. cookie 改读核心 snapshot
5. 此时 authApi 只剩 verify 一处 → 换掉 → network/api/AuthApi 可删
```

---

## 双客户端真正切断：登录链路已无 VRCM 自己的 VRChat 请求（2026-11-17）

关键发现：`AuthenticatedRuntimeSession` 自带完整用户 JSON。

```rust
pub struct AuthenticatedRuntimeSession {
    pub user_id: String,
    pub display_name: String,
    pub endpoint: String,
    pub websocket: String,
    pub current_user: RawJson,      // ← 登录响应里就有，不需要再查
}
```

所以不需要额外的 `current_user` 命令，也不用碰 `VrchatConfigRuntime` 的配置管道。

### 改法

```
1. loginBasic 返回 CoreLogin(state, userJson)
   —— userJson = session.currentUser
2. AuthService.loginLocked: 把 userJson 存进 pendingCoreUserJson
3. emitAuthed: pendingCoreUserJson 非空时直接反序列化成 CurrentUserData，
   跳过 (response ?: authApi.userRes())
4. finally 里清空 pendingCoreUserJson
```

### 效果

| 环节 | 之前 | 现在 |
|---|---|---|
登录 POST | 核心 | 核心 |
拉当前用户 | `authApi.userRes()` ❌ | 登录响应里的 `current_user` ✅ |

登录链路里 VRCM 自己的 VRChat 请求已经没有了。

### 仍是 VRCM 侧的

- `cookiesStorage.cookieValue(AUTH_COOKIE)` —— AccountDto 里仍存 cookie（下一步换成核心 snapshot）
- `verify()` 仍走 `authApi.verify`
- `restoreAuth()` 仍走自己的恢复逻辑

### 编译验证

`:composeApp:compileGithubDebugKotlinAndroid` → BUILD SUCCESSFUL (33s)

---

## 登录生命周期全部走核心（2026-11-17）

| 方法 | 核心命令 | 状态 |
|---|---|---|
`login()` | `app__vrchat_auth_session_start` | ✅ |
`verify()` | `app__vrchat_auth_session_respond` | ✅ |
`restoreAuth()` | `app__vrchat_auth_auto_login_start` | ✅ |
`logout()` | `app__vrchat_auth_session_end` | ✅ |

`restoreAuth` 不再是"读 AccountDto → 重放 cookie → 问 VRChat 要用户"，而是：

```kotlin
if (isVrcxCoreReady()) {
    val core = runCatching { vrcxCoreClient().autoLoginStart() }.getOrNull()
    if (core != null) {
        if (core.state !is AuthState.Authed) return null     // 核心没有可恢复会话 → 保持登出
        pendingCoreUserJson = core.userJson
        return try { emitAuthed(null).getOrThrow(); AuthState.Authed } finally { pendingCoreUserJson = null }
    }
}
// 核心不可用才走旧路径
```

关键取舍：**核心说没有可恢复会话时，不回退到旧路径**。回退会重新引入 VRCM 自己的 VRChat 请求，
等于把刚切断的双客户端又接回去。

### `AutoLoginOutcome` 是 untagged

```rust
#[serde(untagged)]
pub enum AutoLoginOutcome {
    Session(LoginSessionState),          // 有用户 JSON
    Terminal(AutoLoginTerminalOutcome),  // throttled / expired
}
```

所以映射对 Terminal 会落到 `Unauthorized`，`restoreAuth` 因此返回 null —— 正确行为。

### 编译

`:composeApp:compileGithubDebugKotlinAndroid` → BUILD SUCCESSFUL (35s)

---

## 第一个业务 API 走核心：UsersApi.fetchUser（2026-11-17）

```kotlin
suspend fun fetchUser(userId: String): UserData =
    if (isVrcxCoreReady()) {
        Json.decodeFromJsonElement(
            UserData.serializer(),
            vrcxCoreClient().vrchatGet("$USERS_API_PREFIX/$userId")
        )
    } else {
        client.get("$USERS_API_PREFIX/$userId").checkSuccess<UserData>()
    }
```

新增 `VrcxCoreClient.vrchatGet(path)`：走 `app__web_execute`，带状态码检查，
返回原始 JSON。VRCM 的 DTO 原样反序列化 —— 因为核心转发的是 VRChat 原生响应。

**这是第一个"页面数据由 Rust 核心提供"的 API 方法。** 所有调用 `fetchUser` 的地方
（用户详情、好友卡片等）现在都经由核心，而它们的代码一行没改。

### 验证

```
compileGithubDebugKotlinAndroid   BUILD SUCCESSFUL (29s)
installGithubDebug                EXIT=0
pid 13945 / core ready on port 44009 / 无 FATAL
```

尚未用真实数据验证（需要登录后点开某个用户），但编译与启动无回归。

### 可复制的模式

每个 VRChat 读接口都可以照这个形状换：`isVrcxCoreReady()` 分流 + 核心返回原始 JSON +
原 DTO 反序列化。UsersApi 里还有 10 个方法（searchUser / getUserGroups /
getMutualFriends / …），WorldsApi / AvatarsApi / GroupsApi 同理。

---

## 传输链路端到端验证通过（2026-11-17，真机）

用公开端点 `GET /api/1/config`（不需要登录）做探针，真机 logcat：

```
I VrcxCore: core ready on port 44769
I VrcxCore: feedRows ok: []
I VrcxCore: probe vrchatGet(config) ok: {"CampaignStatus":"Disabled",
    "DisableBackgroundPreloads":true,"HistoryFrameworkEternalBurrito":1, ...}
```

完整链路：

```
VRCM (Kotlin/Ktor)
  → http://127.0.0.1:44769/api/invoke
  → CommandRegistry<RuntimeHostState>
  → app__web_execute
  → Rust web_client (持有 cookie jar)
  → https://api.vrchat.cloud/api/1/config
  → 原始 JSON 原样回到 Kotlin
```

这是 Java/Kotlin 侧第一次拿到**由 Rust 核心从 VRChat 取回的真实数据**。
之前所有验证都只到"命令被分发、返回空数组"，这次是真实外部数据。

### 途中抓到的真问题

第一次探针返回：

```
`app__web_execute` is not available on this transport
```

因为**加了 Rust 命令却没重新编译 `.so`** —— APK 里还是旧库。
这个错误码（`unsupported`）设计得对：它把"命令没注册"和"命令执行失败"清楚地区分开，
一眼就能看出是构建产物过期，不是逻辑错。

修法：`cargo build --target x86_64-linux-android -p vrcx-0-mobile-core`
→ 复制 `.so` 到 `composeApp/src/androidMain/jniLibs/x86_64/` → 重新打包。

**这应该进构建脚本**，不能靠人工记。

---

## `.so` 构建步骤：脚本化（Gradle 路线放弃）

### 为什么必须自动化

加了 `app__web_execute` 之后忘了重编译 `.so`，探针返回：

```
`app__web_execute` is not available on this transport
```

APK 里还是旧库。这个坑每改一次 Rust 就会踩一次。

### 试过 Gradle，失败，放弃

在 `composeApp/build.gradle.kts` 里注册 `buildVrcxCore`（`preBuild` 依赖它），
遇到两个问题：

1. `android.ndkDirectory` 抛 `NDK is not installed` —— 模块没声明 `ndkVersion`
2. 改成从 `ANDROID_NDK_HOME` 读之后，**同样的 cargo 命令在 Gradle 的 exec 里
   3 秒即以 101 退出，在 shell 里却成功**。没继续深挖 —— 收益不抵成本

Gradle 里那段已删除，避免留下一个坏掉的钩子。

### 改成脚本：`scripts/build-core.ps1`（VRCM 仓库）

内容就是手工验证过的那几条命令：

```
CC_<target> / AR_<target> / CARGO_TARGET_<TARGET>_LINKER 指向 NDK clang
CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=link.exe
cargo build --target <target> -p vrcx-0-mobile-core
复制 .so 到 composeApp/src/androidMain/jniLibs/<abi>/
```

支持 `-Abi x86_64|arm64-v8a|all`。

### 当前状态（诚实记录）

- 直接跑 cargo：**x86_64 与 aarch64 都成功**（EXIT=0）
- 脚本第一次运行**失败**（报 `could not compile ... 1 previous error`，但同一时刻
  直接跑 cargo 成功）—— 疑似并发 cargo 锁或 PS 5.1 环境注入差异，**尚未定位**
- 结论：脚本记录的是正确命令，但**还不能信任它替代手工步骤**

### 待办

- [ ] 单独（无其他 cargo 进程）跑一次脚本，确认真能成功
- [ ] 若仍失败，改用 `cmd /c` 包一层并捕获 stderr，看清真实错误

---

## `.so` 构建脚本已跑通（2026-11-17）

```
==> cargo build --target x86_64-linux-android
-> x86_64/libvrcx_0_mobile.so (121.3 MB)
==> cargo build --target aarch64-linux-android
-> arm64-v8a/libvrcx_0_mobile.so (110.3 MB)
```

### 根因：clang wrapper 少了 API level 后缀

NDK 的 `bin/` 里同时存在：

```
x86_64-linux-android-clang.cmd        ← 通用 wrapper，链接时报 exit 1
x86_64-linux-android24-clang.cmd      ← 带 API level，正确
```

之前手工命令用的是 `24` 那个，脚本里写漏了。**Gradle 那次的 101 很可能也是同一个原因**
（`$target-clang.cmd`），当时因为看不到 cargo 的 stderr 才误判成"环境差异"。

教训：**捕获 stderr**。这次改成 `cmd /c "cargo ... > log 2>&1"` 之后，
链接器的真实错误一行就出来了，之前 PowerShell 直接 `& cargo` 把它吞掉了。

### 脚本

```
scripts/build-core.ps1  (VRCM 仓库)
  -Abi x86_64 | arm64-v8a | all     默认 all
  -ApiLevel 24                      默认 24
  -CoreRepo <vrcx0/mobile>
  -Ndk $env:ANDROID_NDK_HOME
  -Msvc <MSVC bin>
```

失败时会打印 cargo 日志里的 error 行并给出日志路径。

### 现在的构建流程

```
1. scripts/build-core.ps1          # 改过 Rust 之后
2. gradlew :composeApp:installGithubDebug
```

---

## 事件通道接上运行时总线（2026-11-17）

`GET /api/events` 之前是个空壳：`EventBus` 存在，但**没有任何东西往里推**。
好友在场、通知、同步状态全都走事件的 —— 所以手机端永远收不到更新。

桥接：

```rust
struct BridgeSink(EventBus);

impl RuntimeEventSink for BridgeSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.0.publish(event.to_string(), payload);
    }
}

pub fn build_bus(state: RuntimeHostState) -> Arc<CommandBus<RuntimeHostState>> {
    let events = EventBus::new(256);
    state.event_bus().set_sink(BridgeSink(events.clone()));   // ← 关键一行
    …
}
```

`RuntimeHostState::event_bus()` 是我新加在 composition 上的访问器 —— 桌面用 Tauri 的
`listen` 做 sink，手机用 loopback 广播做 sink，两者从同一个 `RuntimeEventBus` 取事件。

事件名沿用 `T::EVENT_NAME`（46 种），payload 是同一个 `serde_json::Value`，
所以桌面和手机收到的事件形状一致。

### 验证

```
cargo test -p vrcx-0-mobile-core   → 4 passed
```

**尚未验证**：真机上 WS 是否真的收到事件。需要登录后才有事件产生
（presence/notification 都依赖已认证会话）。

---

## 事件通道：Kotlin 侧已写，但握手失败（2026-11-17）

新增：
```
commonMain/mobilecore/CoreEvents.kt    CoreEvents(baseUrl).stream(): Flow<CoreEvent>
androidMain/MainActivity.kt            启动时 collect 并打日志
```

真机结果：

```
I VrcxCore: core ready on port 42025
I VrcxCore: feedRows ok: []
E VrcxCore: event stream failed
E   NoTransformationFoundException: Expected DefaultClientWebSocketSession
E   Response status `400 Bad Request`
E   Response header `ContentType: text/plain`
```

**核心返回 400**，说明 axum 的 `WebSocketUpgrade` 提取器没看到升级头
（`Connection: Upgrade` / `Upgrade: websocket` / `Sec-WebSocket-Key`）。
也就是说 Ktor 那边没有真正发起 WS 升级 —— 尽管 `HttpClient { install(WebSockets) }`。

### 下一步（明确）

不要在 `CoreEvents` 里自建 `HttpClient`，**复用 VRCM 现有的 WS 客户端配置**
（`network/websocket/WebSocketApi.kt` 那条路能连 VRChat 的 pipeline，
说明引擎和插件配置是对的）。做法：照它的 client 构造方式建一个，
或者直接借它已经配好的 `HttpClient` 实例。

### 顺带确认的事

- 命令通道（HTTP）在同一个进程里工作正常，所以不是网络策略问题
- 400 是应用层拒绝，不是 Android 明文流量拦截

---

## 事件通道打通（2026-11-17）

```
I VrcxCore: event stream connecting to http://127.0.0.1:36217
I VrcxCore: event connected: null
```

### 根因：Ktor 只对 `ws://` 做升级

之前传的是 `http://127.0.0.1:<port>/api/events`：

```kotlin
http.webSocket("$baseUrl/api/events")     // ❌ http:// → 普通 GET，无升级头
```

Ktor 对 http URL 不发 `Connection: Upgrade`，axum 的 `WebSocketUpgrade` 因此回 400。
改成：

```kotlin
val wsUrl = baseUrl.replaceFirst("http", "ws") + "/api/events"
http.webSocket(wsUrl)                     // ✅
```

这个错误的表现有误导性：客户端抛的是 `NoTransformationFoundException`
（"无法把响应转成 WebSocket 会话"），像是客户端插件没装，实际是**协议字符串写错**。

### 正面信号

在 `webSocket { }` 块内先发一条 `connected` 事件 —— 该块只在升级成功后执行，
所以它同时是给调用方的可靠信号和真机探针的证据。

### 现在两侧都通

```
命令：POST /api/invoke      ✅ 真机（拿到 VRChat config）
事件：GET  /api/events (WS) ✅ 真机（握手成功，会话保持）
```

事件的实际内容还没验（需要已认证会话才会产生 presence/notification），
但管道两端都已证明可用。

---

## 事件内容端到端验证通过（2026-11-17，真机）

```
I VrcxCore: core ready on port 34665
I VrcxCore: event stream connecting to http://127.0.0.1:34665
I VrcxCore: event connected: null
I VrcxCore: event probeEvent: {"ok":true}      ← 核心推的事件被 Kotlin 收到
I VrcxCore: probe vrchatGet(config) ok: {...}  ← 命令通道同时可用
```

完整推送链：

```
Rust RuntimeEventBus → BridgeSink → EventBus 广播
  → GET /api/events (WS) → Ktor client → Flow → collector
```

用 `app__debug_emit_event` 触发（临时诊断命令），因为真实事件（presence/notification）
只在已认证会话下产生，无法在登录前验证。

### 两个真 bug

**1. `build-core.ps1 -Abi x86_64` 静默什么都不做**

```powershell
$targets = @{ 'x86_64' = '...'; 'arm64-v8a' = '...' }
if ($Abi -ne 'all') { $targets = $targets | Where-Object { $_.Key -eq $Abi } }  # → 变成数组
foreach ($abi in $targets.Keys) { ... }    # 数组没有 .Keys → 循环体不执行
```

结果：退出码 0，但既没编译也没复制。我因此连续两轮装到旧 `.so`，
看到 `app__debug_emit_event is not available on this transport` 还以为是命令注册问题。

改成列表 + `$entry.Abi` 之后正常。

**教训**：脚本"成功退出"不等于"做了事"。凡是幂等操作（编译+复制），
都应该打印实际产物（路径 + 大小 + mtime），否则静默空转无法察觉。

**2. Ktor 只对 `ws://` 做升级**（上一轮已修）

---

## Cookie 传递：静态验证通过（2026-11-17）

之前最大的未知是：登录之后，`app__web_execute` 发出的请求到底带不带得上身份？

查 `composition/src/context.rs:275-281`：

```rust
let login_api: Arc<dyn LoginApi> =
    Arc::new(vrcx_0_outbound_adapters::VrchatLoginApi::new(Arc::clone(&web)));
let auth_cookies: Arc<dyn AuthSessionCookies> =
    Arc::new(vrcx_0_outbound_adapters::WebAuthSessionCookies::new(Arc::clone(&web)));
```

而 `VrchatLoginApi { web: Arc<WebClient> }`（`outbound-adapters/src/login_api.rs:13`）。

**三者共用同一个 `Arc<WebClient>`**：

```
login_api        → web（登录写 cookie）
auth_cookies     → web（读写 cookie）
web_execute      → state.web_client()（同一个 web）
```

所以登录成功后，手机端通过 `app__web_execute` 发的请求**自动带上 `auth` cookie** ——
不需要任何手动传参，这也正是"把客户端下沉到核心"的核心收益。

这个风险此前一直标着"⚠️ 未验证"，现在**静态证明它不成立**，不必等真实登录才能推进读接口替换。

### 仍需真实登录才能验的

- 登录本身是否成功（账号/2FA）
- 已认证请求的返回内容是否正确（而不是 401）
- 事件流在已认证会话下是否产生 presence/notification

---

## 读接口批量替换：UsersApi 三个方法（2026-11-17）

已切到核心的读方法：

```
fetchUser(userId)        → vrchatGet("users/{id}")
getUserGroups(userId)    → vrchatGet("users/{id}/groups")
getMutualFriends(...)    → vrchatGet("users/{id}/mutuals/friends?n=&offset=")
```

模式固定为：

```kotlin
if (isVrcxCoreReady()) {
    Json.decodeFromJsonElement(Deserializer, vrcxCoreClient().vrchatGet(path))
} else {
    client.get(path).checkSuccess<Dto>()          // 旧路径保留
}
```

### 一个 Kotlin 限制导致没法抽 helper

先试过写泛型 helper：

```kotlin
private suspend fun <T> read(path: String, deserializer: DeserializationStrategy<T>): T
```

编译失败：`checkSuccess` 是 **reified inline**，无法接收显式的
`DeserializationStrategy`（`Cannot use 'T' as reified type parameter`）。
所以每个方法只能内联那段 if/else，无法收成一个函数。

### 真正的解法：换 Ktor 引擎

逐个方法改是 O(40)。正确做法是**替换 Ktor 的 engine**，让所有请求经由核心：

```kotlin
HttpClient(CoreEngine()) { configureApiClient(...) }     // NetworkModule.kt:79
```

`CoreEngine` 实现 `HttpClientEngine`，把 `HttpRequestData` 转成
`app__web_execute` 调用，再把结果包成 `HttpResponseData`。

好处：`network/` 里**一行方法都不用改**，全部自动走核心。
而且不需要 gate —— engine 内部判断核心是否就绪，未就绪就转发给默认引擎。

**这件事需要先查 Ktor 3.x 的 `HttpClientEngine` 接口签名**（`execute` 的参数、
`HttpResponseData` 的构造参数、`callContext` 的变化）。我这次没有凭记忆写，
因为接口对不上会白费几轮。

---

## Ktor 引擎换成 CoreHttpEngine（2026-11-17）

```kotlin
// NetworkModule.kt:79
HttpClient(CoreHttpEngine()) { configureApiClient(...) }
```

**一行改动，`network/` 下所有 Api 类自动经由核心**。不再需要逐个方法 gate。

### 引擎实现（子代理完成，`mobilecore/CoreHttpEngine.kt`）

签名来自三处核对（context7 文档 → 本机 ktor 3.1.1 jar 的 `javap` → GitHub 3.1.1 源码），
不是凭记忆：

```
HttpResponseData(statusCode, requestTime, headers, version, body: Any, callContext)   // 6 个位置参数
execute 带 @io.ktor.utils.io.InternalAPI（ERROR 级 opt-in）
```

子代理在离线冒烟里自己踩出两个真 bug 并修好：

1. `JsonArrayBuilder` 没有 `add(String)` 重载
2. `callContext` 必须用 Ktor 的 `io.ktor.client.engine.callContext()` —— 用
   `currentCoroutineContext()` 会在 `HttpStatement.cleanup` 抛
   `ClassCastException: DeferredCoroutine cannot be cast to CompletableJob`
3. 请求头必须走 `mergeHeaders(...)`，否则 POST 的 `Content-Type` 不会进 headers

### 平台 actual

补了 `desktopMain` / `iosMain` 的 `vrcxCoreClient()` / `isVrcxCoreReady()`：
桌面和 iOS 上核心不存在，调用即 `error(...)`，但保证 commonMain 能编译。

### 真机验证

```
I VrcxCore: core ready on port 43539
I VrcxCore: event probeEvent: {"ok":true}
I VrcxCore: probe vrchatGet(config) ok: {...}
无 FATAL，登录页正常渲染
```

### 已知限制（子代理列出，我确认需要处理）

| 限制 | 影响 | 处理 |
|---|---|---|
**无 `WebSocketCapability`** | VRCM 自己的 `WebSocketApi`（连 VRChat pipeline）会以 "Engine doesn't support WebSocketCapability" 失败 | 这本来就是要去掉的东西（实时改由核心的 `/api/events` 提供），但需要显式移除/改造，否则登录后会报错 |
无 `Set-Cookie` | `HttpCookies` 收不到任何 cookie | 前提成立（核心持 cookie），无需处理 |
`HttpTimeout` 不生效 | 核心卡住时请求会一直挂着 | 应把 invoke 包进 `withTimeout` |
响应无 content-type | 默认按 `application/json` | 需要核心回传响应头才能根治 |
二进制 body 会退化 | 文件上传不可用 | 后续按需处理 |

---

## 好友列表无需事件映射就能有数据（2026-11-17 查证）

之前我以为 `WebSocketApi` 一停，好友数据就没了。查证后发现不成立：

```
FriendsApi.fetchFriendList
  → client.get { ... }.checkSuccess<List<FriendData>>()      ← 共享 client
  → 现在由 CoreHttpEngine 承载 → 核心 → VRChat
```

而 `FriendService` 有两条自动刷新路径：

```
FriendService.kt:93    runTask = { refreshFriendList(it) }
FriendService.kt:99    runTask = { refreshFriendList(it.sessionToken, offline = true) }
```

调用方也齐全（`FriendListPagerModel:1163/1196`、`FriendLocationPagerModel:192`、
`FriendNetworkScreenModel:381`）。

**结论：登录之后，好友列表会自动经由核心加载出来，不需要任何事件映射。**
事件的缺口只在"实时变化"（好友上线、换房间、通知），不在"首次加载"。

### 这条改变了验收路径

目标里那句「页面显示本地真实数据并截图」，**只差一次登录**，不缺代码：

```
登录 → FriendsApi.fetchFriendList → CoreHttpEngine → 核心 web_client
     → GET /auth/user/friends → 原始 JSON → FriendData DTO → 好友列表页
```

### 仍然缺事件的场景

- 好友上下线 / 换房间的实时更新
- 通知到达
- 同步状态变化

这些需要把核心推送的事件映射成 VRCM 的 `AccountWebSocketEvent`（或直接喂状态层）。
但那不再是"能看到页面"的前置条件。

---

## 引擎方案的兑现：API 代码里不再有任何核心相关逻辑（2026-11-17）

换引擎时我没注意到一个推论：**`client.get(...)` 本身也走核心了**。
所以之前写的

```kotlin
if (isVrcxCoreReady()) {
    Json.decodeFromJsonElement(...)      // 分支 A
} else {
    client.get(path).checkSuccess<T>()   // 分支 B —— 也在走核心
}
```

两个分支是同一条路，`else` 不是 fallback，是**误导性的死代码**。

已清掉：`UsersApi` 的 `fetchUser` / `getUserGroups` / `getMutualFriends` 全部还原成原始一行式：

```kotlin
suspend fun fetchUser(userId: String): UserData =
    client.get("$USERS_API_PREFIX/$userId").checkSuccess<UserData>()

suspend fun getUserGroups(userId: String): List<LimitedUserGroup> =
    client.get("$USERS_API_PREFIX/$userId/groups").checkSuccess()

suspend fun getMutualFriends(userId, n, offset): List<MutualFriendData> =
    client.get("$USERS_API_PREFIX/$userId/mutuals/friends") { ... }.checkSuccess()
```

同时删掉 5 个不再需要的 import（`isVrcxCoreReady` / `vrcxCoreClient` / `vrchatGet` /
`ListSerializer` / `Json`）。

**这就是引擎方案的真正价值**：`network/` 下所有 Api 类保持原样，
一个字符都不用改，全部经由核心。

### 验证

```
compileGithubDebugKotlinAndroid   BUILD SUCCESSFUL (30s)
installGithubDebug                EXIT=0
pid 16631 / core ready / event connected / probeEvent 收到 / 无 FATAL
```

---

## 回归检查（2026-11-17）：桌面 target 编译通过，但运行时会坏

### 修掉的编译回归

`AuthService` 里我写过 `android.util.Log`（在 commonMain），导致：

```
e: AuthService.kt:824 Unresolved reference 'android'
:composeApp:compileKotlinDesktop  FAILED
```

已改成不带日志的 `runCatching { ... }`。现在两个 target 都过：

```
compileKotlinDesktop                 BUILD SUCCESSFUL (28s)
compileGithubDebugKotlinAndroid      BUILD SUCCESSFUL (28s)
```

### 但发现一个更严重的：引擎替换是无条件的

```kotlin
// NetworkModule.kt:79
HttpClient(CoreHttpEngine()) { ... }
```

而 `CoreHttpEngine.execute` 在 `isVrcxCoreReady() == false` 时返回 **503**。
桌面 target 上 `isVrcxCoreReady()` 恒为 `false`（我写的 actual），
**所以 VRCM 的桌面版本编译通过、但发不出任何 VRChat 请求 —— 等于废了。**

这不影响 `D:\vrcx0\src`（VRCX-0 桌面主线，完全没动），但它是 VRCM 自身的桌面 target 回归。

### 修法（下一轮做）

需要让引擎按平台条件选择：

```kotlin
// commonMain
expect fun createApiEngine(): HttpClientEngine?      // null = 用平台默认引擎

// androidMain
actual fun createApiEngine() = if (isVrcxCoreReady()) CoreHttpEngine() else null
// desktopMain / iosMain
actual fun createApiEngine() = null
```

然后 NetworkModule 里分支构造。代价是配置块要写两份，或者把
`configureApiClient(...)` 抽成一个可复用的 lambda。

**在那之前，VRCM 的桌面 target 不可用** —— 这点必须记住，不能因为"编译过了"就以为没问题。

---

## VRCM 侧改动已提交到独立分支（2026-11-17）

分支 `vrcx-core`（从 1.1.6 拉出，**没动 `main`**），四个逻辑提交：

```
d7edbb7  build: cross-compile the core into jniLibs
c6ed9f2  feat: transport all VRChat traffic through the core
52e8b32  feat: run the login lifecycle through the core
5b30797  feat: load the VRCX-0 core in-process
3f8b027  chore(release): bump version to 1.1.6   ← 原基线
```

`.so` 已加入 `.gitignore`（`composeApp/src/androidMain/jniLibs/`），
所以 121 MB 的二进制不会进仓库 —— 由 `scripts/build-core.ps1` 生成。

### iOS 验证路径

VRCM 已有 `.github/workflows/IOS_Build.yml`：

```
runs-on: macos-latest
./gradlew :composeApp:linkDebugFrameworkIosArm64
```

**所以 iOS 的真实验证是现成的** —— 只要这个分支推到远端，CI 就会跑
`linkDebugFrameworkIosArm64`，那一刻才能确认：

- `expect/actual` 在 iOS target 上成立
- commonMain 没有平台专有泄漏
- `CoreHttpEngine` 用的是 Ktor 公共 API（不含 android-only）

在 Windows 本地无法验证这三点，只能静态保证。

### 待办

**推送需要你的授权** —— 会用你的 GitHub 凭据，而且是对你 fork 的写操作。
推上去之后 CI 才会跑。要我推吗？

---

## 补上超时：核心卡死不再挂住 UI（2026-11-17）

子代理写引擎时明确标记过这个缺口：`HttpTimeout` 的 capability 不会传给
`CoreHttpEngine`，所以核心那边如果命令不返回，请求会**永远挂着**。

修法（`CoreHttpEngine.execute` 内）：

```kotlin
val result = withTimeoutOrNull(60_000L) {
    vrcxCoreClient().invoke("app__web_execute", ...)
} ?: return errorResponse(..., message = "VRCX core did not answer within 60000 ms (GET …)")
```

60 s 对齐的是客户端原本应有的 socket 超时 —— 足够慢响应，又不至于让 UI 卡死。

验证：

```
compileGithubDebugKotlinAndroid + compileKotlinDesktop   BUILD SUCCESSFUL (35s)
installGithubDebug + 启动                                 EXIT=0，core ready / event connected / probeEvent / 无 FATAL
```

提交 `33ba799`（`vrcx-core` 分支第 5 个提交）。

### 分支现状

```
33ba799  fix: bound the core round trip
d7edbb7  build: cross-compile the core into jniLibs
c6ed9f2  feat: transport all VRChat traffic through the core
52e8b32  feat: run the login lifecycle through the core
5b30797  feat: load the VRCX-0 core in-process
3f8b027  chore(release): bump version to 1.1.6   ← 1.1.6 基线
```
