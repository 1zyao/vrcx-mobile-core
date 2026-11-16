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
