# 当前逻辑与问题清单

> 2026-11-17 · 分支 `vrcx-core` · 应要求列出全部逻辑与已知问题

## 一、启动逻辑

```
MainActivity.onCreate
  └→ startVrcxCore(filesDir)                       [androidMain/mobilecore]
       └→ NativeCore.start(filesDir)               JNI，只有这一个跨界调用
            └→ Rust: start_core()                  [crates/mobile-core]
                 ├→ host::build_host()             RuntimeHostStateBuilder + composition
                 │     ├ RuntimeHostProfile::HeadlessData
                 │     ├ local_game_context: Unavailable
                 │     └ group_order / observer / extension: 空
                 ├→ mobile_host::build_bus(host)
                 │     ├ CommandRegistry::build_registry()  10 条命令
                 │     └ state.event_bus().set_sink(BridgeSink)
                 └→ serve_loopback(bus)            127.0.0.1:<随机端口>
        ← 返回 {"port":…, "token":…}
  └→ CoreEvents(baseUrl).stream()                 连 ws://…/api/events
```

## 二、请求逻辑（VRCM 的每一次 API 调用）

```
UsersApi.fetchUser(id)
  └→ client.get("users/{id}").checkSuccess<UserData>()      ← VRCM 原始代码，一行未改
       └→ CoreHttpEngine.execute(HttpRequestData)           ← 自定义 Ktor engine
            ├ 序列化 { url, method, headers, body }
            ├ POST 127.0.0.1:<port>/api/invoke
            │    {"cmd":"app__web_execute","args":{…}}
            └→ Rust: web_client.execute()                   ← 持有 cookie jar
                 └→ https://api.vrchat.cloud/api/1/users/{id}
            ← {"ok":true,"result":{"status":200,"data":"<原样 JSON 字符串>"}}
       └→ HttpResponseData(200, data) → UserData 反序列化
```

**关键点**：`network/` 下 40 多个 Api 方法**没有一行核心相关代码**。换掉引擎就完成了全部改道。

**例外**：图片不经核心（见问题 5）。

## 三、推送逻辑

```
Rust 内部事件（46 种）
  └→ RuntimeEventBus.emit::<T>()
       └→ BridgeSink.emit(event, payload)
            └→ EventBus 广播
                 └→ WS 帧 {"event":"…","payload":{…}}
                      └→ Kotlin CoreEvents.stream() → Flow<CoreEvent>
                           └→ 目前只 println，**还没有接进 VRCM 的 store**
```

## 四、登录逻辑

```
login(username, password)                          [AuthService.loginLocked]
  ├ isVrcxCoreReady() → vrcxCoreClient().loginBasic()      app__vrchat_auth_session_start
  │     args { mode:"basic", username, password, saveCredentials }
  │     ← LoginSessionState
  │         ├ Authenticated → AuthState.Authed + currentUser JSON
  │         ├ Challenge     → NeedEmailCode / NeedTFA / NeedTTFA + attemptId
  │         └ Failed        → Unauthorized(reason)
  ├ pendingCoreUserJson = 用户 JSON（登录响应里自带，不再二次请求）
  └ emitAuthed() → 簿记照旧（AccountDao / SharedFlowCentre），用户数据用核心给的

verify(password, code, cardPage)                   [AuthService.verify]
  └ loginRespond(attemptId, method, code)          app__vrchat_auth_session_respond
       method: EmailCode→emailOtp / TFACode→totp / TTFACode→otp

restoreAuth() → autoLoginStart()                   app__vrchat_auth_auto_login_start
logout()      → logout()                           app__vrchat_auth_session_end
```

## 五、问题清单（按性质分类，共 10 个）

### A. 契约形状靠猜（4 个，都造成了实际故障）

| # | 问题 | 症状 | 根因 |
|---|---|---|---|
1 | `{"input": {...}}` 多包一层 | `missing field \`mode\`` | 没确认 `register_typed` 是把整个 args 反序列化 |
2 | 枚举 tag 大小写 | `login -> Unauthorized attempt=null`（收到验证码却报未知错误） | `rename_all="camelCase"` 输出小写 `challenge`，我按大写匹配 |
3 | `HttpTimeoutCapability` 未声明 | 登录成功后第一个请求抛 `IllegalArgumentException` | Ktor 要求引擎声明 capability，我以为是"超时不生效"这种温和问题 |
4 | `attemptId` 字段来源 | 2FA 无法继续 | 同 #2，解析失败导致 attemptId=null |

### B. 传输层的真实限制（2 个，属设计取舍但代价明确）

| # | 问题 | 影响 | 处理 |
|---|---|---|---|
5 | `web_execute` 返回 `data: String` | **二进制退化 → 头像/图片全坏** | 已改用独立客户端给 Coil，绕过核心 |
6 | 引擎不声明 `WebSocketCapability` | VRCM 自己的 VRChat WS 无法工作 | 已停止它（实时改由核心提供），但 VRCM 侧的事件消费还没接 |

### C. 构建与工具事故（4 个，全是我操作层面）

| # | 问题 | 后果 |
|---|---|---|
7 | 改了 Rust 没重编译 `.so` | 两次装到旧库，报"命令不存在"，我误判为注册问题 |
8 | `build-core.ps1` 用 Hashtable 过滤 | `-Abi x86_64` 静默什么都不做、退出码 0 |
9 | NDK clang wrapper 缺 API level | 链接失败，且 Gradle 那次因此被误判成"环境差异"，白花数轮 |
10 | PowerShell 正则/`Replace` 误伤源码 | 4 次：吃掉泛型、反引号未转义、匹配错位置、大小写不敏感去重删掉 import |

## 六、诚实的结论

这 10 个问题里：

- **真正的设计错误只有 2 个**（#5 二进制、#6 WS）—— 而且都是子代理事先预警过、我低估了严重性的
- **4 个是"没先验证契约就接线"**（A 类）—— 而验证成本极低：一条 curl、一行 `println` 就能发现
- **4 个是我自己的编辑/构建操作事故**（C 类）—— 与架构无关

所以"问题好多"是真的，但**大部分不是架构问题，是我把验证顺序做反了**：先写代码，再靠真机试错发现契约错误。正确的顺序应该是每个新命令先 curl 打到真进程上，确认形状，再接 UI。

这条教训在 #1~#3 上重复了三次，代价是你手动试了两次登录。

## 七、现在真正能跑的部分（真机验证）

```
✅ 登录 + 邮箱 2FA 成功        日志：verify -> Authed hasUser=true
✅ 用户数据来自核心            emitAuthed coreUser=true（无二次请求）
✅ 好友/收藏数据加载            "77 favorites loaded"，realtimeFriendProjection 有内容
✅ 核心接管 VRChat 实时 WS      realtimeWsStatus: connecting
✅ 命令总线（手机 / headless）  两侧真请求验证
✅ 事件通道（手机 / headless）  两侧真请求验证
✅ headless 代理命令           curl 拿到真实 config
```

## 八、还没做的

```
⬜ 实时事件接进 VRCM 的 store   现在只打日志，好友上下线不会刷新 UI
⬜ VRCM 已有的「好友动态」页接 VRCX 的 feed（app__feed_rows_query 已可用）
⬜ network/ 清理                  Api 类可删，DTO 保留
⬜ iOS 编译验证                  需 push 触发 CI
⬜ app__debug_emit_event 移除     诊断用
```

---

# 新目标第一轮：后端读模型验证通过（2026-11-17）

## 验证方式

复制桌面 VRCX-0 的真实库（596 MB，不动原件）到 `out\logs\feed-probe\`，
用 headless 命令总线查询：

```
$ vrcx-0-headless --data-dir <copy> --command-bus-port 8892 --command-bus-only
POST /api/invoke
{"cmd":"app__feed_rows_query",
 "args":{"userId":"usr_ed329568-…","mode":"lookup","maxEntries":5}}
```

## 结果

```
lookup -> rows=5
{
  "avatarName": "Warhammer 40,000: Space Marine 2 - Ultramarine Redemptor Dreadnought",
  "created_at": "2026-11-16T20:46:12.456526200+00:00",
  "currentAvatarImageUrl": "https://api.vrchat.cloud/api/1/image/file_999cfbbc-…/1/256",
  "displayName": "Kemomimi Yuki",
  "ownerId": "usr_05447869-…",
  ...
}
```

`mode=lookup` 与 `mode=search` 都返回真实行。

## 这证明了什么

1. **后端的语义命令能给出真实读模型数据** —— 不是空数组、不是透传的 VRChat JSON
2. **返回的字段正好覆盖 VRCM 需要的形状**：

```
FeedRowOutput                    FriendActivityEvent
displayName                  →   displayName
currentAvatarImageUrl        →   profileImageUrl
created_at                   →   occurredAtMillis
type                         →   type（6/9 可映射）
worldName / location         →   worldName / currentValue
avatarName                   →   currentValue（Avatar 类）
```

3. **图片 URL 是 `https://api.vrchat.cloud/api/1/image/...`** —— 公开 CDN，
   UI 用普通客户端直接取即可（不经命令总线，天然避开二进制退化）

## 两个踩到的小坑（都是我的调用方式，不是系统问题）

- `args` 必须传对象，不能传 JSON 字符串（PowerShell `ConvertTo-Json` 没展开）
- `userId` 必填，否则报 `User table prefix requires a user id`（feed 按用户前缀分表）

## 下一轮

```
1. VrcxCoreClient 支持可配置 baseUrl（后端地址）
2. 加 feedQuery(userId, mode, maxEntries, cursor) 便捷方法
3. 写 BackendFriendActivityTimelineSource 实现 observeHead/Before/Through
4. 接进 FriendActivityTimelineModel（页面不动）
5. 真机看动态
```

## 尚未验证（必须诚实标注）

这份数据来自**桌面端自己的库**，不是 remoteSync 把它们同步到后端的结果。
所以「后端作为同步消费者」这条链路**仍未被验证** —— 它和「读模型可用」是两件事。
新目标第（2）条的验收，需要后端有经同步得来的数据才能成立。

---

# 第 2 轮：VRCM 作为独立后端的纯前端（拓扑基础验证）

## 做了什么

**1. 可配置后端地址（`VrcxBackend`）**

```kotlin
object VrcxBackend {
    @Volatile var baseUrlOverride: String? = null   // 例如 http://192.168.31.84:8891
    val isRemote: Boolean
}
```

`vrcxCoreClient()` 的 Android actual 现在：

```kotlin
VrcxBackend.baseUrlOverride?.let { return VrcxCoreClient(it, token = "") }   // 远端后端优先
return requireNotNull(endpoint) { "no VRCX backend configured …" }
```

配了地址 → 进程内核心完全不参与。这正是目标 (1) 要求的形态。

**2. `feedQuery(...)`** —— 语义命令的客户端封装

```kotlin
suspend fun VrcxCoreClient.feedQuery(
    userId: String, mode: String = "lookup",
    maxEntries: Int = 60, search: String = "", cursor: String? = null
): JsonElement
```

## 拓扑验证（关键证据）

在主机上跑独立后端：

```
vrcx-0-headless --data-dir <真实库副本> --command-bus-port 8891 --command-bus-only
```

- 主机本机：`POST /api/invoke` → **200**
- 模拟器 `ping 10.0.2.2` → 通（0.597 ms）
- 模拟器 `busybox wget --post-data` → **拿到真实响应**：

```json
{"ok":true,"result":{"authenticatedRuntimePhase":{"authScopeGeneration":0,
 "endpoint":"","favorites":{"status":"pending"},"friends":{"status":"pending"},
 "phase":"idle",...}}}
```

也就是说：

```
MuMu 模拟器 ──HTTP──> 10.0.2.2:8891 ──> 主机上的独立 VRCX 后端 ──> CommandRegistry
```

**手机端不需要进程内核心**。目标 (1) 的拓扑成立了。

## 编译

```
:composeApp:compileGithubDebugKotlinAndroid   BUILD SUCCESSFUL (27s)
```

## 下一轮

```
1. FriendActivityTimelineSource 的 Backend 实现
   —— 需要 FriendActivityTimelineCursor 的字段与 AccountSessionToken 的构造方式
2. 接进 FriendActivityTimelineModel（页面不动）
3. 后端地址来源：先读 files/backend.url（调试用），后续做设置页
4. 真机看到动态
```

## 诚实标注

- ✅ 拓扑：手机 → 独立后端（已验证，真请求）
- ✅ 读模型：`app__feed_rows_query` 返回真实行（第 1 轮验证）
- ❌ 同步：后端持有的是**桌面库的副本**，不是 remoteSync 同步来的
- ❌ UI：`FriendActivityTimelineSource` 的 Backend 实现还没写，页面上还看不到
