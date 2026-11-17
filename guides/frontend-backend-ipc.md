# VRCX-0 前后端通信（给接手会话的最小交接）

源码在 `D:\vrcx0\src`（分支 `sync-main`）。这份文档只讲**前端 React/TS 与后端 Rust 怎么互相调用**，
以及加一个新接口要动哪些地方。

---

## 0. 一句话

**前端不写传输代码，后端不写暴露代码。** 后端把命令和事件在一处登记，
`tauri-specta` 据此生成 `src/platform/tauri/bindings.ts`（命令签名 + 全部 TS 类型），
前端只调 `commands.<域><动作>()`。

```
前端                                    src-tauri（薄壳）                crates/*（真正逻辑）
────                                   ─────────────────              ─────────────────
commands.syncStatus()
  └→ __TAURI_INVOKE("sync__status") ──▶ sync__status(State<AppState>)
     (@tauri-apps/api/core.invoke)        #[tauri::command(async)]  ──▶ state.runtime_host()
                                          #[specta::specta]             .remote_sync().status()
  ◀── Promise<SyncStatusSnapshot>              └─ Result<T, AppError>（Err → reject）

listen("realtimeProjectionSync")
  ◀── TauriRuntimeEventSink.emit()   ◀── event_bus.emit(RealtimeFeedProjection { .. })
  events.ts → dispatch → store
```

---

## 1. 命令通道（前端 → 后端，当前 **617 条**）

### 后端定义一个命令

`src-tauri/src/commands/local/sync.rs:112`

```rust
#[tauri::command(async)]
#[specta::specta]
pub async fn sync__status(state: State<'_, AppState>) -> Result<SyncStatusSnapshot, AppError> {
    Ok(state.runtime_host().remote_sync().status().await)
}
```

- 命名约定 `域__动作`（`sync__`、`app__`、`storage__`…），TS 侧自动变成 `camelCase` 方法名
- Rust 的**位置参数**在 JS 侧合并成**一个对象**（`{ enabled, connection, intervalSec, collectorPriority }`）
- 阻塞型工作（SQLite）走 `tauri::async_runtime::spawn_blocking`，别在 async 里直接锁库

### 登记与生成

```
src-tauri/src/bindings_export.rs:109   pub fn builder() -> Builder<tauri::Wry>
                   :111                .error_handling(ErrorHandlingMode::Throw)
                   :141                .commands(collect_commands![ ...617 条... ])
                   :762                pub fn export_bindings()
src-tauri/src/app.rs:493               .invoke_handler(bindings_export::builder().invoke_handler())
```

`ErrorHandlingMode::Throw` 很关键：Rust 的 `Err(AppError)` 会在 JS 侧变成 reject，而不是返回 `{ok:false}`。

重新生成绑定（**改了 `crates/**` 的契约就要重跑**）：

```powershell
cd D:\vrcx0\src
$env:TEMP='D:\vrcx0\tmp'; $env:TMP='D:\vrcx0\tmp'      # 必须！否则写临时文件报 os error 5
$env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'
cargo build --bin export-bindings --features=bindings-export
.\target\debug\export-bindings.exe
npx --no-install oxfmt src/platform/tauri/bindings.ts
npm run typecheck
```

### 前端调用

```
src/platform/tauri/bindings.ts:3     import { invoke as __TAURI_INVOKE } from './generatedInvoke'
src/platform/tauri/generatedInvoke.ts 错误归一化（写错误日志 + 触发 SQLite 错误事件）
src/platform/tauri/invoke.ts          最底层 = @tauri-apps/api/core 的 invoke
```

业务代码里不要直接 `invoke('xxx__yyy')`，用生成的 `commands.xxxYyy(...)`，
并且封装在 `src/repositories/*Repository.ts` 里（例如 `syncRepository.ts`），
组件不直接碰 `commands`。

---

## 2. 事件通道（后端 → 前端，单向广播）

### 发出：业务代码只管往事件总线丢

```rust
self.event_bus.emit(RealtimeFeedProjection { generation, owner_user_id, upserts, patches });
```

事件名由类型常量决定（`crates/application-core/src/event_bus.rs:145`）：

```rust
runtime_event_payload!(RealtimeProjectionSync, "realtimeProjectionSync");
```

### 桥接：`src-tauri/src/bootstrap/adapters.rs:49`

```rust
impl RuntimeEventSink for TauriRuntimeEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        log_gui_background_runtime_info(&self.app_handle, event, &payload);
        emit_to_main_window_if_visible(&self.app_handle, event, payload);   // 窗口不可见时不推
    }
}
```

> 注意：因为「不可见就不推」，**切到 tray / 最小化期间的事件不会自动补发**；
> 前端重新可见时要靠 snapshot 类命令补齐（各 store 的 hydrate 逻辑）。

### 接收：`src/platform/tauri/events.ts`

```
:12  const listeners = new Map<string, Set<TauriEventHandler>>()
:16  lazy import('@tauri-apps/api/event').listen
:29  dispatch(name, payload) → 各订阅者
```

订阅者集中在 `src/services/runtime-event-bridge/`，把事件翻译成 store 的 patch。

---

## 3. 加一个新接口要动哪三步

**新命令**

```
1. crates/<合适的 crate>     写真正的业务方法
2. src-tauri/src/commands/… 加 #[tauri::command(async)] #[specta::specta] 包装
3. src-tauri/src/bindings_export.rs 的 collect_commands![…] 里登记
   → 重跑 export-bindings → 前端多出 commands.xxxYyy()
```

**新事件**

```
1. crates/… 定义 struct + impl RuntimeEventPayload（EVENT_NAME 常量）
2. 在 bindings_export.rs 的类型清单里登记（否则 TS 侧没类型）
3. crates/… 里 event_bus.emit(YourPayload { .. })
4. 前端 src/services/runtime-event-bridge/ 里订阅并落到 store
```

---

## 4. 平台差异（Cloud / 网页版参考）

同一个「命令名 + 载荷」协议，传输层可以换掉：

```
桌面   POST 概念 = Tauri invoke         事件 = Tauri event
Cloud  POST /api/invoke {cmd,args}      事件 = WS /api/events
       → 前端只换 src/platform/native/webTransport.ts，617 个命令一个都不用改
```

`D:\vrcx0\cloud` 是 Cloud 的独立 worktree（2.30.0 扁平拷贝），可以当作「同一份前端跑在浏览器」的参照。

---

## 5. 已知坑（都会咬人）

| 现象 | 原因 / 处理 |
|---|---|
| `export-bindings` 报 `Failed to write … .raw.ts: 拒绝访问 (os error 5)` | `%TEMP%` 权限问题 → 先把 `$env:TEMP` 指到 `D:\vrcx0\tmp` |
| `bindings.ts` 改了没生效 / 被覆盖 | 它是生成物；只改它会被下次导出覆盖，改契约才是根源 |
| `cargo build --release -p vrcx-0` 出的 exe 白屏、连 `localhost:9000` | 没走 `tauri build`，前端资源没嵌入 → 必须 `npx tauri build`（可加 `--no-bundle`） |
| 构建极慢（10~15 分钟） | `[profile.release]` 是 `lto = true` + `codegen-units = 1`（发布档，别为了迭代改它）。改界面用 `npx tauri dev` |
| 链接器 | 用 `set CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=C:\Program Files\LLVM\bin\lld-link.exe` |
| `openvr_sys` 构建报 `LNK1104: 无法打开 …openvr_api64.lib` | 搬目录后 cmake 缓存失效 + 并行抢写；先单独 `cmake --build target\release\build\openvr_sys-*\out\build --target openvr_api64 --config MinSizeRel`，再正常构建 |

完整环境与构建说明见 `D:\vrcx0\README.md`。

---

## 6. 文件速查

```
前端
  src/platform/tauri/bindings.ts          生成的命令签名 + 全部类型（勿手改）
  src/platform/tauri/generatedInvoke.ts   调用包装（错误归一化）
  src/platform/tauri/invoke.ts            最底层 invoke
  src/platform/tauri/events.ts            事件订阅 / dispatch
  src/services/runtime-event-bridge/      事件 → store 的翻译层
  src/repositories/*Repository.ts         命令的业务封装（组件只调这层）

后端
  src-tauri/src/app.rs:493                挂 invoke_handler
  src-tauri/src/bindings_export.rs        命令清单 + 事件类型清单 + export
  src-tauri/src/commands/**               命令实现（按域分文件）
  src-tauri/src/bootstrap/adapters.rs:49  事件总线 → WebView 的桥
  crates/application-core/src/event_bus.rs  event_bus + runtime_event_payload! 宏
  crates/composition/src/state/**          RuntimeHostState：真正被命令调用的对象
```
