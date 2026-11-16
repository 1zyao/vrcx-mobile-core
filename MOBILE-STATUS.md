# VRCX-0 × VRCM 状态报告

> 快照时间：2026-11-17 · 分支 `vrcx-core`（VRCM 1.1.6 基线）

## 一、目标架构（已落地）

```
┌─ 手机 APK：VRCM 的 Kotlin/Compose 工程 ────────────────────────┐
│  VRCM 页面（presentation/ + designsystem，一行未改）            │
│       ↓ Ktor                                                    │
│  CoreHttpEngine  ← 自定义 Ktor engine，所有 API 请求的出口        │
│       ↓ HTTP                                                    │
│  http://127.0.0.1:<ephemeral>/api/invoke      ← 前后端边界        │
│  ws://127.0.0.1:<ephemeral>/api/events        （推送方向）        │
│       ↑ JNI 仅用于启动一次                                        │
│  libvrcx_0_mobile.so   ← VRCX-0 Rust 核心（进程内）               │
│    · SQLite 本地库                                              │
│    · 共享 WebClient（cookie jar）→ VRChat API                    │
│    · RuntimeEventBus → 46 种事件                                 │
└────────────────────────────────────────────────────────────────┘

桌面 / 服务器：同一份 CommandRegistry，另一种绑定
  headless --command-bus-port <p> [--command-bus-only]
      → 固定端口，可被远程客户端访问
```

**一份注册表，两个宿主。** 手机用随机端口 + 进程内；服务器用固定端口 + 可远程。

## 二、验证矩阵（真进程 / 真请求 / 真实数据）

| 能力 | 编译 | 启动 | 真请求 | 真实数据 | 证据 |
|---|---|---|---|---|---|
命令总线（手机） | ✅ | ✅ | ✅ | ✅ | 真机 logcat：`probe vrchatGet(config) ok: {...}` |
命令总线（headless） | ✅ | ✅ | ✅ | ✅ | curl `app__backend_runtime_combined_snapshot_get` → 200 + 真实快照 |
代理命令（手机） | ✅ | ✅ | ✅ | ✅ | 同上，返回 VRChat 原生 JSON |
代理命令（headless） | ✅ | ✅ | ✅ | ✅ | curl → `ok=true, status=200` + 真实 config body |
事件通道（手机） | ✅ | ✅ | ✅ | ✅ | 真机 logcat：`event probeEvent: {"ok":true}` |
事件通道（headless） | ✅ | ✅ | ✅ | ✅ | 标准 .NET `ClientWebSocket` 收到同名帧 |
| 登录 / 2FA | ✅ | — | ❌ | ❌ | **需要一次人工登录** |
| iOS 编译 | ⚠️ 仅静态 | — | — | — | **需要 push 触发 CI** |

### 复现命令

```powershell
# 1. 交叉编译核心（改过 Rust 之后必须跑）
cd D:\vrcm\vrcm-base
$env:ANDROID_NDK_HOME = 'C:\Android\Sdk\ndk\27.3.13750724'
powershell -File scripts\build-core.ps1 -Abi x86_64 -CoreRepo D:\vrcx0\mobile

# 2. 打包装机
$env:JAVA_HOME = 'C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot'
gradlew.bat :composeApp:installGithubDebug

# 3. 看核心是否起来
adb -s 127.0.0.1:16384 logcat | Select-String 'VrcxCore'

# 4. headless 命令总线
cd D:\vrcx0\mobile
cargo build -p vrcx-0-headless
target\debug\vrcx-0-headless.exe --data-dir <dir> --command-bus-port 8899 --command-bus-only
# 另一个终端：
#   POST http://127.0.0.1:8899/api/invoke {"cmd":"app__web_execute",
#        "args":{"url":"https://api.vrchat.cloud/api/1/config","method":"GET"}}
```

## 三、命令与事件面

**已注册（10 条）**

```
app__feed_rows_query                      本地 SQLite 的 feed 读模型
app__vrchat_auth_session_start            登录
app__vrchat_auth_session_respond          2FA 应答
app__vrchat_auth_session_cancel           取消
app__vrchat_auth_session_end              登出（返回 saved snapshot）
app__vrchat_auth_saved_snapshot_get       会话恢复用的凭据快照
app__vrchat_auth_auto_login_start         从保存凭据恢复
app__backend_runtime_combined_snapshot_get 运行时相位 + 会话投影
app__web_execute                          通用 VRChat 代理（替代 41 条逐端点命令）
app__debug_emit_event                     诊断用，正式版应移除
```

**为什么不搬那 41 条**：VRCM 自己知道要调哪 94 个端点，Rust 侧不需要理解每个响应结构；
`app__web_execute` 返回 `{status, data}`，VRCM 现有 DTO 原样反序列化。

**事件**：`RuntimeEventBus` → `BridgeSink` → 广播 → `/api/events`（WS）。
事件名沿用 `T::EVENT_NAME`（46 种），payload 与桌面一致。

## 四、关键设计决定（及理由）

1. **换 Ktor 引擎，而不是逐个改 Api 方法**
   O(1) 而非 O(40)：`network/` 下所有 Api 类保持原样，一行未改即全部走核心。
   （Kotlin 的 `checkSuccess` 是 reified inline，无法抽泛型 helper，逐个改只能内联重复代码。）

2. **引擎在 Android 上无条件使用**
   Koin 建 client 早于核心启动，若在此处 gate `isVrcxCoreReady()`，会永久退回平台引擎、
   静默绕开核心。启动瞬间的请求收到 503 —— 可见且诚实。

3. **`web_execute` 一条顶 41 条**
   看/查类端点的响应形状由 VRCM 的 DTO 负责，Rust 只转发。

4. **登录/2FA/恢复/登出全走核心；`emitAuthed` 保留簿记**
   一次性删掉会让 20+ 依赖方立刻失去登录态。只换 HTTP 那一处，簿记照旧。

5. **cookie 由核心持有**
   已验证 `VrchatLoginApi` / `WebAuthSessionCookies` / `web_execute` 共用同一个
   `Arc<WebClient>` —— 登录写入的 cookie 自动被代理请求带上。

6. **引擎不支持 WebSocket 是刻意的**
   VRCM 自己那条 VRChat pipeline WS 已停用，实时改由核心事件提供。

## 五、已知缺口与风险

| 项 | 状态 | 影响 / 处理 |
|---|---|---|
登录 / 2FA 实测 | ❌ 未做 | 好友列表能否出现全看这一次 |
iOS 编译 | ⚠️ 仅静态 | 需 push 触发 `IOS_LinkBuild.yml`（`linkDebugFrameworkIosArm64`） |
`--command-bus-only` 之外的 headless | ⚠️ | 无账号时进程会退出（原行为） |
`HttpTimeout` | ✅ 已补 | 引擎内 60 s `withTimeoutOrNull` |
响应无 content-type | ⚠️ | 默认按 JSON；非 JSON 端点会失败，需核心回传响应头才能根治 |
二进制 body | ⚠️ | 文本化会损坏，文件上传不可用 |
`HttpCookies` 收不到 Set-Cookie | 设计如此 | cookie 全在核心 |
`app__debug_emit_event` | ⚠️ 临时 | 正式版应移除 |
`network/` 目录 | 未清理 | Api 类仍在（但已被引擎接管），DTO 仍需要 |

## 六、仓库状态

```
D:\vrcx0\src        9e9fe29ac  干净，未动（VRCX-0 桌面主线）
D:\vrcx0\mobile     8954 个文件，本报告所在仓库（Rust 侧全部改动）
D:\vrcm\vrcm-base   分支 vrcx-core，6 个提交（VRCM 侧全部改动）
```

`vrcx-core` 提交序列：

```
93150e5  chore: diagnostics for the core-backed login paths
33ba799  fix: bound the core round trip
d7edbb7  build: cross-compile the core into jniLibs
c6ed9f2  feat: transport all VRChat traffic through the core
52e8b32  feat: run the login lifecycle through the core
5b30797  feat: load the VRCX-0 core in-process
3f8b027  chore(release): bump version to 1.1.6     ← 基线
```

## 七、下一步（按依赖排序）

1. **人工登录一次** —— 诊断日志已就位，一条命令即可读出结果
   ```
   adb logcat -c && <点登录> && adb logcat | Select-String 'VrcxCore'
   ```
2. **push 分支** —— 触发 iOS CI，验证 expect/actual 与 commonMain 无平台泄漏
   ```
   git -C D:\vrcm\vrcm-base push -u github-vrcx vrcx-core
   ```
3. 事件接进 VRCM 的 store（登录后才有事件可接）
4. `network/` 清理：删 Api 类，只留 DTO 与路径常量
5. 移除 `app__debug_emit_event`，补 `HttpTimeout` 之外的引擎 capability

---

## 追加（2026-11-17 晚，第 75 轮）

### 运行时证据：restoreAuth 的核心分支在真机上执行

```
I System.out: [VrcxCore] restore -> Unauthorized hasUser=false
```

证明三件事：`println` 在 Android 上确实进 logcat；`restoreAuth` 走的是
`app__vrchat_auth_auto_login_start`；`AutoLoginOutcome → AuthState` 映射在无凭据时
正确落到 `Unauthorized`。

### 但引擎路径仍未观测到

logcat 里没有 `[VrcxCore] proxy ...` 行 —— VRCM 在登录前不发任何请求。

### 尝试用 adb 自动登录：失败

想用假账号证明引擎路径（`POST auth/user` 会打出 proxy 行），试了两轮：

```
adb shell input tap 540 1193     # 点用户名框
adb shell input text "probeuser"
```

结果字段仍是占位符，`dumpsys input_method` 显示：

```
mInputShown=false
mServedView=DecorView{...}[MainActivity]     ← 焦点停在窗口而非输入框
```

两次截图的 sha256 完全一致（连状态栏时钟都没变），说明屏幕完全没响应输入。

**结论：Compose 的文本框无法用 `adb shell input` 驱动（至少在这个构建里）。**
所以登录验证必须人工点击，这也解释了我此前一直无法自行完成这一格验证。
