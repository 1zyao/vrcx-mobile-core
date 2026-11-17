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

---

# 第 5 轮：耦合面核查 + 规范命令名（关键结构发现）

## 一、核查结果（改了计划）

```
presentation 文件总数            276
直接 import network.api.*        96   ← 35%
被使用的 Api 类                  16 个
AvatarModerationApi, AvatarsApi, EconomyApi, FavoriteApi, FileApi, GroupsApi,
InstancesApi, InventoryApi, InviteApi, NotificationApi, PlayerChatboxModerationApi,
PlayerModerationApi, PrintsApi, ProfileAppearanceApi, UsersApi, WorldsApi
```

**结论：`service/` 不是唯一出口。** 原计划"重写 service 层即可"是错的。

### 修正后的分层决定

保留 `network/api/*` 作为**适配层**（它实际上就是 VRCX 文档里 `repositories/` 的角色）：

```
presentation/            一行不改（96 个文件保持原样）
network/api/XxxApi.kt    方法体从「发 VRChat 请求」改成「调语义命令」
service/                 逐域替换
VrcxCoreClient           = webTransport.ts（纯 invoke + 事件）
```

比"重写 96 个 presentation 文件"风险低一个数量级，且符合"名字/载荷即协议"。

## 二、规范命令名（照抄 cloud，不再自造）

```
app__social_friend_roster_baseline_get     好友名单基线
app__social_unfriend / _selection
app__social_friend_request_send / cancel / notification_accept
app__current_user_refresh
app__friend_profile_load_start / _cancel
app__friend_log_names_resolve / _cancel
app__user_groups_overview_get
app__vrchat_auth_current_user_get
app__feed_rows_query / app__feed_latest_query / app__feed_search_query
```

## 三、发现一个必须先解决的形状错配

VRCM 的 `FriendsApi.fetchFriendList` 期待**一次调用返回完整列表**（`GET auth/user/friends`）。

而 VRCX 的模型是：

```
app__social_friend_roster_baseline_get   → 基线快照
+ realtimeFriendProjection（事件）        → 后续增量 patch
```

**没有"给我好友列表"这一条命令。** 所以适配 `FriendsApi` 不是换个 URL 那么简单，
需要在适配层做一次"基线 + patch"的合并，或者 fork 里补一条返回扁平列表的读模型命令。

这个决定要你拍板：

- **方案 A**：适配层自己合并（`roster_baseline` 拿到基线，事件流里挑 patch 应用）
  —— 不改后端，但适配层变厚
- **方案 B**：fork 里加一条 `app__friends_flat_get`（后端拼好扁平列表）
  —— 适配层薄，但**新增了规范里没有的命令**（与你刚强调的原则冲突）

我倾向 **A**，因为它不新增协议。

## 四、本轮没有代码改动

只有这份核查结论。下一步先 curl 验证 `app__social_friend_roster_baseline_get`
的真实返回结构，再决定 A 的合并逻辑。

---

# 第 7 轮：决定「原数据层丢弃」+ 迁移清单

## 决定（用户明确）

> 就是要让 VRCM 变成那个前端，它原来的数据层丢弃。

所以 `network/` **不是适配层，是要被删除的层**。上一轮我提的"保留 Api 类只改方法体"
被否掉。

## 已做

`FriendActivityTimelineModel` 在配置了独立后端时改用后端读模型：

```kotlin
constructor(service: FriendActivityService) : this(
    if (VrcxBackend.isRemote) BackendFriendActivityTimelineSource(vrcxCoreClient())
    else ServiceFriendActivityTimelineSource(service)
)
```

编译通过（28s）。这是第一个「页面数据来自后端读模型」的接点。

## 迁移清单（按调用点数量排序，决定动手顺序）

```
FriendsApi                     3 个文件   ← 但需要 social 服务移植
AvatarModerationApi            3 个文件
AuthApi                        4 个文件
EconomyApi                     4 个文件
PrintsApi                      4 个文件
FavoriteApi                    5 个文件
PlayerChatboxModerationApi     5 个文件
NotificationApi                6 个文件
InventoryApi                   6 个文件
InstancesApi                   7 个文件
ProfileAppearanceApi           7 个文件
PlayerModerationApi            7 个文件
AvatarsApi                    11 个文件
WorldsApi                     14 个文件
GroupsApi                     14 个文件
FileApi                       15 个文件
InviteApi                     15 个文件
UsersApi                      23 个文件
```

`network/api/` 下共 **22 个 Api 类**。

## 动手顺序（不是按数量，而是按「后端是否已有对应能力」）

**第 1 批（后端已有，纯 db 读）**
```
feed        ✅ app__feed_rows_query 已验证 → 已接线
favorites   app__* favorites 相关（待 curl 验证）
avatars     app__* avatar 相关（待 curl 验证）
```

**第 2 批（需要移植 social / moderation 服务）**
```
friends     app__social_friend_roster_baseline_get
notifications app__* notification 相关
groups      app__user_groups_overview_get
```

**第 3 批（写操作，风险最高，最后做）**
```
InviteApi / FileApi / 各种 moderation 动作
```

## DTO 的处置（目标里的另一条约束）

「契约尽量由 Rust 类型生成 Kotlin DTO，不手写同步」——
`network/api/*/data/*` 里的 DTO 目前是 VRCM 手写的。随着各域迁移，
这些 DTO 应换成从 Rust 契约生成的类型（feed 的 `FeedRowOutput` 就是先例：
我现在是手工解析 JSON，长期应改为生成类型）。

这一条我还没做，先记着，等第 1 批迁完后评估生成方案。

---

# 第 8 轮：目标确认 + 一个硬事实

## 决定（用户明确）

> VRCM 原来的那一套 对 VRChat API 的各种请求、登录的那一套，都废弃。
> 让 VRCX 成为后端，而不是一个兼容层。

所以终態是：

```
删 network/                      （22 个 Api 类 + Ktor 客户端配置 + cookie 存储）
删 CoreHttpEngine                （连二进制/WS/超时那三个坑一起消失）
删 AuthService 的本地会话簿记    （AccountDao / cookies / emitAuthed）
留 presentation/                 数据改由命令 + 事件驱动
留 VrcxCoreClient                = webTransport.ts
```

VRCM 不再有任何"发请求给 VRChat"的代码，也不再自己维护登录态。

## 硬事实（本轮查出来的）

**18 个域里，现在只有 feed 能服务。**

| 检查项 | 结果 |
|---|---|
fork 的 host 有 `local_data()` | ❌ 没有 |
fork 的 host 有 `social()` | ❌ 没有 |
`feed_rows_query` | ✅ 对 db 的**自由函数**，所以能用 |
`favorite_list` | 只在 `application-realtime/src/ports.rs` 作为 **trait 方法**存在 |
cloud 的 `local_data()` 门面 | 不在 `cloud/crates/server/src` 里（在上层 assembly） |

也就是说：**除了 feed，其它域都要先把 VRCX 的服务端门面（local_data / social）移植进 fork，
命令才能注册。** 这不是"改调用点"能解决的，是移植工作。

## 因此关键路径（不再改计划）

```
第 1 步：把 favorite_list 这条最小的端到端切片打通
        a. 在 fork 里找到/移植 favorites 的读门面（application-realtime 的 trait 已有定义）
        b. 实现一个最小的 host 门面（只做 favorites）
        c. 注册 app__favorite_list，args = { kind }
        d. curl 打真进程，用 596MB 真实库验证返回
第 2 步：改 FavoriteApi 的 5 个调用点 → 删 FavoriteApi
第 3 步：按同样套路做 avatars → worlds → groups → friends → notifications
第 4 步：全部迁完，删 network/ 与 CoreHttpEngine 与 AuthService 簿记
```

## 本轮没有代码改动

只是在决定方向后核实了「哪些域现在能用」。结论是只有 feed，
所以必须先做门面移植 —— 这是绕不过去的一步，也是我之前没做的部分。

---

# 第 10 轮：avatars 域的映射审计（覆盖面 10/11）

## 方法

从 cloud 全部命令注册里抽出**去重命令名**：**361 条**（与早前 bindings 的数字一致；
文档里的 617 是桌面 `collect_commands!` 的条目数，含重复与包装）。

然后逐条比对 VRCM `AvatarsApi` 的 11 个方法：

| VRCM | VRCX 规范命令 | 备注 |
|---|---|---|
`getAvatarStyles` | `app__vrchat_avatar_styles_get` | 读 |
`getAvatars` | `app__vrchat_avatar_list_by_user_get` | 读 |
`getAvatarById` | `app__my_avatar_by_id_get` | 读 |
`getFavoritedAvatars` | `app__favorite_list {kind:"avatar"}` | ✅ 已在本 fork 验证过 |
`selectAvatar` | `app__vrchat_avatar_select` | 写 |
`selectFallbackAvatar` | `app__vrchat_avatar_select_fallback` | 写 |
`updateAvatar` | `app__vrchat_avatar_save` | 写 |
`deleteAvatar` | `app__vrchat_avatar_delete` | 写 |
`enqueueImpostor` | `app__vrchat_avatar_impostor_create` | 写 |
`deleteImpostor` | `app__vrchat_avatar_impostor_delete` | 写 |
`getImpostorQueueStats` | **无对应** | VRCX 不提供 |

## 这改变了工作量判断

之前我以为"每个域都要先移植服务端门面"。对 **写操作** 类的方法不成立：

VRCM 的 Api 大多是 VRChat REST 的薄包装，而 VRCX 有对应的 `app__vrchat_*` 命令
（`vrchat_mutations.rs`，41 条）。这些命令本身也是对 VRChat 的调用，只是被 VRCX
规范化、并挂在它的运行时上。

所以移植目标分两类：

```
本地读模型   app__feed_rows_query / app__favorite_list / app__avatar_get …
             → 纯 db 读，已在 fork 可用（feed、favorites 已验证）

VRChat 动作  app__vrchat_avatar_select / _delete / _styles_get …
             → 需要把 vrchat_mutations 里对应的那条搬进 fork（形状照抄）
```

## 下一轮

挑**最小的一条**先搬：`app__vrchat_avatar_styles_get`（只读、无参数依赖），
搬进 fork 后用 curl 打真进程验证返回 JSON，再继续下一条。

## 顺带确认

`app__favorite_list {kind:"avatar"}` 就是 `getFavoritedAvatars` 的实现方式 —— 
所以 favorites 那条命令不只服务收藏页，也服务头像页。一个命令覆盖两处需求。

---

# 第 11 轮：`app__vrchat_avatar_styles_get` 的移植定位（未完成）

## 已完成：定位

```
cloud 的实现：crates/server/src/commands/vrchat_proxy.rs:341
    "app__vrchat_avatar_styles_get",
    |state, _args| async move { ok(state.vrchat_remote().avatar_styles().await?) },

门面本体（fork 里就有）：crates/application/src/remote/vrchat_facade.rs
    pub struct VrchatRemoteFacade { api: VrchatApiRuntime, … }
    pub async fn avatar_styles(&self) -> Result<VrchatResponse>          // :336

请求构造函数（fork 里也有）：
    crates/application/src/remote/vrchat_proxy/avatars.rs:96
    pub fn avatar_styles_get_input(endpoint: String) -> VrchatRequest
```

所以命令面、门面、请求构造器**三者 fork 都具备**，缺的只是**把门面接进 host**。

## 卡点（下一轮从这里开始）

`VrchatRemoteFacade` 的构造：它的字段是 `api: VrchatApiRuntime`（私有），
而 fork 里搜不到 `VrchatFacade::new` / `VrchatRemoteFacade::new` 的调用点 ——
说明它是由 composition 在内部构造的（cloud 那边通过 `state.vrchat_remote()` 暴露）。

因此要给 fork 加：

```
1. RuntimeHostContext / RuntimeHostState 上的 vrchat_remote() 访问器
   （或一个最小的 facade 构造：VrchatApiRuntime 需要 web_client + 配置）
2. 然后 app__vrchat_avatar_styles_get 只是 3 行注册
3. curl 验证
```

## 这一轮的价值

把"能不能搬写操作类命令"这个问题答清楚了：

- 命令名与参数形状：**有**（cloud 里现成）
- 门面实现：**fork 已有**（`vrchat_facade.rs`）
- 缺的只有 **host 上的一个访问器 + 一个构造**

也就是说 41 条 `vrchat_mutations` / `vrchat_proxy` 里的命令，移植成本集中在
**先把 facade 接进 host 这一处**，之后每条命令都是几行。

## 下一步（明确）

```
a. 读 VrchatApiRuntime 的构造要求（需要哪些依赖）
b. 在 composition 里构造 VrchatRemoteFacade 并挂到 RuntimeHostState
c. 注册 app__vrchat_avatar_styles_get
d. curl 打真进程验证
e. 之后批量搬其它 app__vrchat_* 命令
```

---

# 第 12 轮：真机跑通"登录 + 真实数据"，全部经由核心（关键里程碑）

## 证据一：会话从核心恢复

```
[VrcxCore] restore -> Authed hasUser=true
[VrcxCore] emitAuthed coreUser=true
```

`restoreAuth` 调的是 `app__vrchat_auth_auto_login_start`，凭据在核心的库里，
一次调用就恢复成已认证；`emitAuthed` 用的是核心返回的用户 JSON，没有二次请求。

## 证据二：引擎承载了全部 VRChat 业务请求

```
proxy GET https://api.vrchat.cloud/api/1/auth/user
proxy GET https://api.vrchat.cloud/api/1/auth/user/friends?offline=false&offset=0&n=50
proxy GET https://api.vrchat.cloud/api/1/favorites?type=friend&offset=0&n=50
proxy GET https://api.vrchat.cloud/api/1/favorite/groups?n=50&offset=0&type=friend
proxy GET https://api.vrchat.cloud/api/1/notifications?limit=100
proxy GET https://api.vrchat.cloud/api/1/auth/user/notifications?type=friendRequest&hidden=false
proxy GET https://api.vrchat.cloud/api/1/profile/usr_ed329568-…?with=…
```

这些是 VRCM **原来的 Api 方法**发出的请求，全部被 `CoreHttpEngine` 接住并转给核心，
核心用自己的 cookie jar 发出去。运行期没有再出现 `HttpTimeoutCapability` 之类的异常
（上一轮声明的 capability 生效了）。

## 证据三：界面显示真实数据

首页截图（`out\android\shot-logged-in.png`）：

- 自己的资料：`97323  33  ask me`（在线好友 33）
- `处于房间内(7)` —— 好友按房间分组，7 个房间
- 房间卡片带世界缩略图（`咖啡厅v1.31` 的图片正常显示 → Coil 改独立客户端生效）
- 房间内成员头像正常（`Kemomimi Yuki`、`宅萝林宝`）

也就是说：**登录、好友列表、房间分组、收藏、通知，全部经由核心拿到，界面正常渲染。**

## 这个里程碑意味着什么，以及它还不是什么

**是**：VRCM 作为 VRCX 前端的可行性已被证实 —— 传输层、登录、数据获取、图片都通了。

**还不是**：数据路径仍然是 `app__web_execute` **透传 VRChat REST**。
按目标要求，这个必须废弃，改成语义命令。也就是说：

```
现在：（VRCM Api 方法）→ 引擎 → 核心 → 透传 URL → VRChat
目标：（VRCM Api 方法）→ 核心语义命令 → 核心自己决定怎么回答（本地库 / 同步数据 / VRChat）
```

用户对这一步的原话：**「VRCM 原来的那一套对 VRChat API 的各种请求、登录的那一套，都废弃」**。

## 也因此确认了拓扑问题

用户答：「手机自己自足，手机只有一个 VRChat 客户端，也就是 VRCX 核心」。

所以 `vrchat_remote()` 加在**共用 host** 上是正确的：手机进程内的核心就是手机侧
唯一的 VRChat 客户端，Compose 前端永不直连。已把这条决定同步给正在做接线的子代理。

（本轮 `su` 不可用，`backend.url` 没写进设备，所以 app 走的是进程内核心 ——
正好就是用户要的形态。）

---

# 第 18 轮：配额的真实来源（查 VRCX 前端得出）

## 问题

VRCM 的收藏页显示 `avatars1 24/50` 这样的配额，来自 `GET /auth/user/favoritelimits`。
这条在 VRCX 的命令面里**没有对应**，我上一轮提了三个选项 (a) 保留 proxy / (b) 从组数据推算 / (c) 不显示。

## 查证：VRCX 前端怎么拿配额

```
D:\vrcx0\src\src\state\favoriteStore.ts:101
    favoriteLimits: cloneFavoriteLimits(snapshot.favoriteLimits),
```

→ 前端**不请求** favoritelimits，它从运行时快照里取。

核心侧的证据：

```
crates/application-realtime/src/social_baseline/types.rs
    pub struct FavoriteBaselineSnapshot {
        pub favorite_limits: RawJson,              ← 配额
        pub favorite_friend_groups: Vec<FavoriteGroupOutput>,
        pub favorite_world_groups:  Vec<FavoriteGroupOutput>,
        …
    }

    pub struct FavoriteGroupOutput {
        pub key, type_name, name, display_name, visibility,
        pub capacity: i64,                         ← 每组上限（50）
        pub count:    i64,                         ← 已用（24）
    }
```

```
crates/application-realtime/src/social_baseline/service/favorites.rs
    build_favorite_groups_from_limits(&favorite_limits)   ← 核心自己把 limits 摊成组+capacity
    MAX_FAVORITE_GROUPS_KEY     = "maxFavoriteGroups"
    MAX_FAVORITES_PER_GROUP_KEY = "maxFavoritesPerGroup"
```

## 结论：三个选项都不对，正确答案是第四个

配额是 **social baseline 的一部分**：核心自己取 `/auth/user/favoritelimits`，
把 `maxFavoritesPerGroup` 摊成每组的 `capacity`，连同 `count` 一起放进基线快照。
所以手机端应该读 `app__social_favorites_baseline_get`，而不是自己调 limits。

这也意味着 `24/50` 这个显示**能精确还原**（`count`/`capacity`），不需要估算。

## 顺带看清的一件事：剩余 proxy 请求收敛到同一个缺失件

```
GET /auth/user/friends         → app__social_friend_roster_baseline_get
GET /auth/user/favoritelimits  → app__social_favorites_baseline_get
（大概率）GET /notifications    → 同族的 notification 服务
```

三个域都依赖 **social baseline 服务**，而 fork 里没有它（cloud 在
`crates/runtime-host-server/src/social.rs`，`ServerSocialRuntime`，14 个依赖字段）。

所以下一步不是"逐条补命令"，而是**一次把 social 运行时移植进 fork**，
然后 friends / favoritelimits / 通知 三块可以一起翻。

---

# 第 27 轮：还剩多少直连（源码级审计）

运行时日志只能证明"点过的页面没走 proxy"，源码审计能列出**所有**仍可能直连的调用点。
下表是 `composeApp/src/commonMain/kotlin/network/api/` 里仍在用 `client.get/post/put/delete` 的方法，
它们经 `CoreHttpEngine` → `app__web_execute` 透传，运行时就表现为
`proxy GET https://api.vrchat.cloud/...`。

## 已清零（运行时验证：0 条 proxy）

| 域 | 方法 |
|---|---|
| 登录/会话 | `auth_session_*`、`auto_login_start`、`current_user` |
| 好友 | `friendsFlow`（roster baseline + presence 合并） |
| 收藏 | `fetchFavorite`、`getFavoriteGroupsByType`、`getFavoriteLimits`、`deleteFavorite` |
| 用户 | `fetchUser`、`fetchUserRaw`、profile 读取 |
| 群组 | `fetchGroup` |
| 实例 | `instanceByLocation` |
| 通知 | `fetchNotifications`、`fetchNotificationsV2` |
| 动态 | `feed_rows_query`（本地读模型） |

## 仍走透传的方法（按文件）

```
AuthApi              approveDevice, deviceAuthInfo, isAuthed, userRes, verify
AvatarModerationApi  blockAvatar, getAvatarModerations, unblockAvatar
AvatarsApi           deleteAvatar, deleteImpostor, enqueueImpostor, getAvatarById, getAvatars,
                     getAvatarStyles, getFavoritedAvatars, getImpostorQueueStats, selectAvatar,
                     selectFallbackAvatar, updateAvatar
EconomyApi           getCreditsBalance
FavoriteApi          addFavorite, clearFavoriteGroup, updateFavoriteGroup
FeedbackApi          reportConfig, submitReport
FileApi              deleteFile, findImageFileLocal, getAvatarGalleryFiles, getFileInfo, getFiles
FriendsApi           deleteFriendRequest, sendFriendRequest, unfriend
GroupsApi            getGroupGalleryImages, getGroupInstances, getGroupMembers, getGroupPosts,
                     getUserGroupInstances, joinGroup, leaveGroup, searchGroups,
                     updateGroupNotifications, updateRepresentation
InstancesApi         closeInstance, closeStatus, createInstance
InventoryApi         getInventory, getTemplate, redeemReward
InviteApi            getInviteMessages, inviteMyselfToInstance, inviteUser, requestInvite,
                     resetInviteMessage, updateInviteMessage
NotificationApi      acceptFriendRequest, clearAllNotifications, deleteNotification,
                     deleteNotificationV2, markLegacyNotificationAsRead,
                     markPipelineNotificationAsRead, responseNotification
Player*ModerationApi get, moderate, remove / getForTarget
PrintsApi            deletePrint, getPrint, getUserPrints
ProfileAppearanceApi update
UsersApi             addTags, blockUser, boop, createPlayerInteractionOverride,
                     deleteWorldPersistence, getMutualFriends, getPlayerInteractionSnapshot,
                     getRepresentedGroup, getUserGroups, hasWorldPersistence, isUserBlocked,
                     removePlayerInteractionOverride, removeTags, saveUserNote, unblockUser,
                     updateUserInfo
VrchatStatusApi      fetchStatus
WorldsApi            deleteWorld, getFavoritedWorlds, getRecentWorlds, getWorldById,
                     getWorldInstanceById, getWorldPublishStatus, publishWorld, searchWorld,
                     unpublishWorld, updateWorld
```

## 下一步的顺序（按页面触发频率）

1. **世界详情** `getWorldById` → `app__world_get`（VRCX 前端用的就是它，本地读模型；fork 缺实现，正在移植）
2. **群组成员/公告/画廊** → `app__vrchat_group_members_get` / `_posts_get` / `_gallery_get`
3. **模型列表/详情** → `app__vrchat_avatar_get` / `app__avatars_by_user`（门面已有）
4. **搜索世界** → `app__vrchat_search_worlds_get`（门面已有）
5. **写操作**（好友请求、邀请、收藏增删、封禁…）→ 用规范写命令，不是读模型

写操作要格外小心：它们是真正会改账号数据的，验证只能用"反序列化阶段失败"的方式（见第 15 轮的做法），不能拿真实账号试。

---

# 第 37 轮：读域清零，写操作仍未接

## 全站巡检（真机，一次走完）

页面：首页（位置 + 动态）、收藏（玩家/世界/模型/群组 4 个 tab）、通知、搜索、房间/世界页。

```
VRChat proxy 请求：0 条
异常（VrcxCoreException / FATAL / 序列化 / MissingField）：无
```

房间卡片、收藏列表、通知列表、动态列表均正常渲染。所以"手机端不直接请求 api.vrchat.cloud
的业务端点"这一条，**在**这些页面上成立，且是运行时证据，不是代码推断。

## 已经由核心提供的读（各轮 curl + 真机验证）

| 域 | 命令 | 验证方式 |
|---|---|---|
| 动态 | `app__feed_rows_query` | 本地读模型，顶部为 04:46 的真实记录 |
| 通知 | `app__notification_list_query` | curl 10 条真实数据 |
| 好友 | `app__social_friend_roster_baseline_get` | count=69 |
| 收藏 | `app__social_favorites_baseline_get` | 77 条 `remoteFavoritesById` |
| 收藏详情（模型） | `app__favorite_details_hydrate` | `requestedIds` 触发抓取；真机模型 tab 有内容 |
| 收藏详情（世界） | `app__vrchat_favorite_worlds_get` | curl 2 条含 favoriteId/favoriteGroup |
| 世界 | `app__world_get` | summary / full 两态 |
| 世界搜索 | `app__vrchat_search_worlds_get` | 200，含 `releaseStatus=all` → 403 的坑 |
| 实例 | `app__vrchat_instance_get` | world=Kyrravik users=3 |
| 用户 | `app__vrchat_user_get` / `user_profile_get` | 200 |
| 用户群组 | `app__vrchat_group_user_groups_get` / `user_represented_group_get` | 57 个群组 |
| 群组 | `app__vrchat_group_get` | 200 |
| 模型列表 | `app__vrchat_avatar_list_by_user_get` | Salt、白云_EFT |
| 玩家屏蔽 | `app__local_moderation_list` | 0 条（该库无记录） |
| 文件元数据 | `app__file_metadata_get` | id / name / ownerId |
| 文件分析 | `app__vrchat_auth_file_analysis_get` | `variant=security` → fileSize 343682971 |
| 标记已读 | `app__notification_mark_seen_batch` | 无会话时给可读错误 |

注册表：37 条命令。

## 仍未接的 94 个调用点（全部是"动作/写"或未移植的域）

```
AuthApi                approveDevice, deviceAuthInfo, isAuthed, userRes, verify
AvatarModerationApi    blockAvatar, getAvatarModerations, unblockAvatar
AvatarsApi             deleteAvatar, deleteImpostor, enqueueImpostor, getAvatarById, getAvatarStyles,
                       getImpostorQueueStats, selectAvatar, selectFallbackAvatar, updateAvatar
EconomyApi             getCreditsBalance
FavoriteApi            addFavorite, clearFavoriteGroup, getFavoriteGroupsByType, updateFavoriteGroup
FeedbackApi            reportConfig, submitReport
FileApi                deleteFile, findImageFileLocal, getAvatarGalleryFiles, getFileInfo, getFiles
FriendsApi             deleteFriendRequest, sendFriendRequest, unfriend
GroupsApi              getGroupGalleryImages, getGroupInstances, getGroupMembers, getGroupPosts,
                       getUserGroupInstances, joinGroup, leaveGroup, searchGroups,
                       updateGroupNotifications, updateRepresentation
InstancesApi           closeInstance, closeStatus, createInstance
InventoryApi           getInventory, getTemplate, redeemReward
InviteApi              getInviteMessages, inviteMyselfToInstance, inviteUser, requestInvite,
                       resetInviteMessage, updateInviteMessage
NotificationApi        acceptFriendRequest, clearAllNotifications, deleteNotification,
                       deleteNotificationV2, responseNotification, mark*AsRead (旧路径已不再被模型调用)
PlayerChatboxApi / PlayerModerationApi   moderate, remove, getForTarget
PrintsApi              deletePrint, getPrint, getUserPrints
ProfileAppearanceApi   update
UsersApi               addTags, blockUser, boop, createPlayerInteractionOverride,
                       deleteWorldPersistence, getMutualFriends, getPlayerInteractionSnapshot,
                       getRepresentedGroup(旧), hasWorldPersistence, isUserBlocked,
                       removePlayerInteractionOverride, removeTags, saveUserNote, unblockUser,
                       updateUserInfo
VrchatStatusApi        fetchStatus
WorldsApi              deleteWorld, getRecentWorlds, getWorldPublishStatus, publishWorld,
                       unpublishWorld, updateWorld
```

## 为什么停在这里

剩下的几乎都是**写操作**（加好友、发邀请、收藏增删、封禁、发公告…）。它们的验收方式与读不同：
不能拿真实账号去执行，只能验证"参数形状 + 无会话时的可读拒绝"，成功路径要用户亲自触发。

继续做的正确顺序是：先挑**低风险且可重复**的写（收藏增删、加入/退出群组），再挑**不可逆**的
（解除好友、封禁、删世界）。不可逆的那些，移植后必须由用户点一次确认。

## 已知的语义缺口（不是 bug，是取舍）

- 群组读走的是原始读，不是 VRCX 的 groups 服务：手机端没有群组级缓存。
- 收藏世界不支持 `search/sort/featured/...`：命令没有这些参数，传了会**报错而不是静默忽略**。
- 玩家屏蔽的非空映射未在真机验证：该账号库中没有屏蔽记录。

---

# 第 38 轮：验收标准 (2) 的两半，分别是什么状态

目标里那条验收标准是：「在后端/桌面产生的事件，能在手机 VRCM 上看到」。

它其实是两件事，必须分开看，否则会把"能显示"当成"能同步"。

## 已证明：读模型能把"别人产生的事件"顶到最前

在数据库副本里插入一条带未来时间戳的 GPS 事件，然后调 `app__feed_rows_query`：

```
rows=10
   type=GPS at=2026-11-17T12:00:00.000000000+00:00 user=ProbeRecorder world=Kyrravik   ← 插入的
   type=Avatar at=2026-11-16T20:46:12.456526200+00:00 user=Kemomimi Yuki
   type=GPS at=2026-11-16T20:46:02.453609400+00:00 user=绅士般的骗子 world=云海眠居0․21․2
```

插入的事件排第一位，顺序、内容都对。所以**只要事件进了库，UI 就会显示** —— 这一半是通的。

数据库现状也印证同一条链路：`..._feed_gps` 表 **132,779 行**，最新 `2026-11-16T20:46:02`，
手机 UI 顶部显示的 04:46 就是这条（时区 +8）。这些行是**桌面录制**产生的，手机上一条 VRChat
请求都没发。

## 未实现：把事件从桌面搬到手机的这个"搬运"

手机能看到那些行，是因为它的数据库是**从桌面库复制过去的**，不是同步来的。

而且复制不能替代同步，这一点有直接证据（更早已记录）：把一个完整的 SQLite 文件复制到手机后，
首次打开给出 `restore -> Unauthorized` —— 因为凭据在 `files/VRCX-0/secrets.key` 里，不在库里。

所以验收标准 (2) 的当前状态是：

```
读模型：✅ 通（本轮实验 + 132,779 行真实数据）
搬运：❌ 没有实现（现在是文件复制，不是核心间同步）
```

## 这意味着什么

"手机端不直接请求 api.vrchat.cloud 的业务端点"这条已经成立（全站巡检 0 条 proxy）。
但"数据来自 VRCX 的远端同步"这条，**成立的是数据来源的合法性，不是传输机制**。

要让手机 7x24 跟上桌面，缺的是核心间同步协议：凭据协商（不是复制 secrets.key）、增量拉取、
冲突处理。这是下一个真正的里程碑，比继续翻写操作更重要。

## 第 39-51 轮：同步已通，读域收口，一次失败与一次误报

上一节末尾那句"搬运没有实现"是错的，这里更正并补上后续。

### 更正：同步一直是实现的

VRCX 的 remoteSync 是 **PostgreSQL 共享库**（`config:vrcx_remotesync.*`），`crates/application-sync`
+ `crates/outbound-adapters/src/remote_sync_postgres.rs` 早就在。缺的只有一件事：手机的
`start_core()` 从没调 `start_data_services()` —— 桌面（`state.rs:1502`）和 headless
（`startup.rs:165`）都调了，手机没有。

数据库直接给出证据：

```
修复前：sync.lastCycleAt = 2026-11-16T20:46:30Z   pullCursor = 838977（= 复制来的桌面库，从未变过）
修复后：sync.lastCycleAt = 2026-11-17T00:11:44Z   pullCursor = 839096（+119 拉取）
```

`contracts/src/sync.rs` 的 `_feed_gps` 模板说明动态表本就在同步集合里。

### 读域收口（命令 42 → 50）

```
群组页   group_get / members_get / posts_get / user_instances_get / gallery_get   → 页面 0 proxy
模型     avatar_get（本仓库新增命名）/ avatar_styles_get
最近访问 改为读游戏日志（见下）
```

`app__vrchat_avatar_get` 是我们自己加的名字，代码里写明了原因：cloud 的 `app__avatar_get`
是**本地缓存读**，cloud 读单个模型走列表或相册命令。请求体仍是协议层的 `avatar_get_input`。

### 最近访问：一次失败，然后换对方向

第一次尝试保留世界卡片、逐行 `app__world_get` 补缩略图 —— 真机一直转圈。原因是
`app__world_get(full=false)` **缓存未命中会回落网络**，50 张卡片等于最多 50 次 VRChat 请求，
而原来的实现只有 1 次。已回退，原因写在 `WorldsApi.getRecentWorlds` 上方。

第二次改成渲染录制里真有的东西（世界名 / 时间 / 时长 / 同场人数），页面 **0 proxy**：

```
来发会呆吧        11-13 23:24 · 42 分钟 · 1 人
Let me decay      11-13 23:17 · 6 分钟 · 8 人
Travel to me.     11-11 21:14 · 1 小时 21 分钟 · 2 人
```

### 一次必须记住的误报

我曾报告 `app__game_log_sessions_query` 返回空世界名，并断言是持久层映射 bug。**结论是错的**：
会话 DTO 是 `#[serde(rename_all = "camelCase")]`，线上字段是 `worldName`，我读的是 `world_name`。
唯一读对的 `created_at` 恰好有显式 `#[serde(rename = "created_at")]`，让错误假设看起来自洽。

诚实记录这件事，是因为它差点导致去改一段**桌面和手机共用且正确**的持久层代码。

### 第 52 轮：全站回归 sweep（15 步）

```
首页(位置) 0 · 首页(动态) 0 · 收藏(玩家/世界/模型/群组) 0 0 0 0
通知 0 · 搜索页 0 · 世界页 0
群组页(详情/帖子/成员/相册) 0 0 0 0 —— 两个不同群组(皇家女仆团 / CHOCOLAT)
最近访问(录制) 0 · 设置·隐私与数据(同步卡片) 0
个人资料页(群组/创建的世界/创建的模型/收藏的世界 均有内容) 0
```

### 仍然没有规范命令的域

cloud 对下列域**没有命令**，移植等于自己造名字：库存、图库/打印、邀请消息、共同好友（cloud 的
`app__user_mutual_friends_list_get` 读的是本地 mutual graph，不是 HTTP）。
这些页面的透传仍在（`UsersApi` 14 处、`AvatarsApi` 7 处、`WorldsApi` 6 处、`InviteApi` 6 处…），
其中绝大多数是写操作。

## 判据（2）的精确状态：搬运通了，但搬来的不是动态

第 52 轮用数据库对账（手机库 vs 当初复制的那份 `probe-19`），结论必须写清楚，因为它比"同步通了"更细。

### 成立的部分

```
_sync_row_version   418,690 → 418,692      (+2 行)
sync.pullCursor     838,977 → 839,149      (+172)
cycleHistory        每 15s 一轮，反复出现 {"pulled":2,"pushed":2,"error":null}
sync.lastPullAt     2026-11-17T00:56:27Z
```

最新写入的 `_sync_row_version` 条目（按 HLC 倒序），手机库 vs 复制件：

```
手机库：
  configs                     config:vrcx_savedcredentials / vrcx_lastuserloggedin
  usred32…_profile_bio        usr_73f1a771-…
  usred32…_self_profile_log   ["2026-11-16T22:57:33.833Z","bio",""]

复制件：
  usred32…_feed_avatar        ["2026-11-16T20:46:12.456526200+00:00", …]
  usred32…_feed_gps           ["2026-11-16T20:46:02.453609400+00:00", …]
```

复制件里最新的是 **20:46** 的动态行；手机库里有 **22:57** 的 bio 事件。也就是说，**有别的设备在往共享库写，手机确实把它拉进来了。**

### 还没成立的部分

动态四张表与复制件**逐表完全一致**：

```
feed_gps     132,779 行  最新 2026-11-16T20:46:02  （两边相同）
feed_status   17,768 行  最新 2026-11-16T19:53:32  （两边相同）
feed_bio       1,691 行  最新 2026-11-14T09:40:02  （两边相同）
feed_avatar  104,816 行  最新 2026-11-16T20:46:12  （两边相同）
```

所以：**同步机制通了、搬的是真数据（config + bio 事件，时间比复制件更新），但"一条新的动态事件经同步出现在手机上"这件事还没有被观察到** —— 因为远端本身自 20:46 之后就没有新的动态行。桌面端的录制停了。

这也意味着：今天手机上的 13 万条动态，主体来自当初的**文件复制**，同步此后补的是 config/profile 这类表。判据（2）的"机制"成立，"动态经同步流入"待桌面恢复录制后确认。

### 想看它发生，需要什么

1. 桌面端 VRCX 保持运行并录制（产生新的 feed 行）
2. 手机端每 15s 拉一轮
3. 观察：手机 `feed_gps` 的 `MAX(created_at)` 与行数**在复制件之上**增长，且 UI 动态页顶部出现该条

我没有通过"往共享库里插一条假动态"来制造这个证据 —— 那会污染用户在所有设备上的真实动态流。

### 一个坑：feed 表名带 owner 前缀

```
usred329568c23b4ee1aec9e902f29dff19_feed_gps
```

直接写 `feed_gps` 会报 `no such table`；库里的 owner 前缀是用户名 sub 去横线的形式。

## 第 53-57 轮：只读域收尾，以及一个解释了三轮补丁的根因

### 命令面 50 → 57

```
只读    inventory_items_get（云已有，未注册）· media_files_get（云已有，未注册）
        media_prints_get / media_print_get（云已有，未注册）· economy_balance_get（自命名）
写      avatar_select / avatar_select_fallback（自命名，可逆）
读模型  game_log_sessions_query
```

这轮有个反复出现的模式：**门面里早就有 cloud 的规范实现，只是没在手机端注册**。
`media_prints_get`、`media_print_get`、`media_inventory_items_get` 都是这样。我两次差点又造
新名字（`app__vrchat_prints_get`、`app__vrchat_inventory_get`），都是 grep 到已有实现才纠正。
**新增命令前先查门面和注册表。**

### 最重要的发现：两个 JSON 解码器不一致

翻完 `/files` 后，图库 Gallery 从 **4/64 变成 0/64**，界面上没有任何错误，只有日志里一行：

```
Field 'animationStyle' is required for FileData, but it was missing
```

原因：VRChat 对相册图片不返回 `animationStyle`，而

```
应用网络层 Json：coerceInputValues = true, explicitNulls = false
CoreJson（命令总线）：只有 ignoreUnknownKeys + isLenient
```

同一份 payload，走 Ktor 能解，走命令总线就抛。

**这解释了第 26、35 轮为什么给 `WorldData` / `UnityPackage` 逐字段加默认值** —— 那不是设计，
那是按 DTO 打补丁。现在 `CoreJson` 配置对齐，同类问题不会再一个个冒出来。

教训：一个 DTO 在一条路径上能解、另一条路径上不能，说明问题在解码器，不在 DTO。

### 库存页：一个"拒绝"差点拒绝掉正常路径

`archived = false`（界面默认「归档: 当前」）我第一版直接拒绝，页面报「无法加载库存」。
但**省略该参数在网络语义上就等于"不含归档"**，所以 `false` 应当省略，只有 `true` 才拒绝。
截图抓到了这个错误；只看编译不会。

### 写操作的选择标准

这轮只加了 `avatar_select`，理由写在提交里：**可逆（切回即可），且只影响自己**。
解除好友、封禁、删除世界/模型、群组管理这些没有做 —— 它们真正的风险是用用户账号对别人做事，
而我能验证的只有"无会话时报错"这一层。

### 第 58 轮：终态回归 sweep

见下方"终态"一节。
