# 核心侧（mobile 分支）跟进 VRCX 主线更新复杂度评估

## 执行摘要

**复杂度评分**：**7/10**（中高风险）

**核心问题**：
1. **31 个共享 crates 的手工同步**（无自动化工具）
2. **16 个 crates 已产生分支差异**（+5KB 到 -3KB 不等）
3. **架构移植不完整**（mobile 独立实现了 4 个 crates）

**最大风险**：主线重构某个核心 crate 时，mobile 分支会面临**三向合并**（主线更新 + mobile 定制 + 冲突解决）

---

## 一、架构分析：mobile 分支的组成

### 1.1 Workspace 结构对比

```
主线 (src/Cargo.toml)          mobile (mobile/Cargo.toml)
├─ 34 个 crates               ├─ 35 个 crates（+1 新增）
│  ├─ runtime-host-desktop    │  ├─ runtime-host-desktop (保留)
│  ├─ composition             │  ├─ composition (已修改，+1文件)
│  └─ 其他 32 个               │  ├─ social-runtime (新增)
                               │  ├─ command-server (新增)
                               │  ├─ mobile-host (新增)
                               │  ├─ mobile-core (新增)
                               │  └─ 其他 31 个（与主线共享）
```

**关键事实**：
- **31 个 crates 与主线同名**：`application`, `persistence`, `composition` 等
- **4 个 mobile 独有 crates**：
  - `social-runtime` — 从主线 `runtime-host-server` 移植的 social baseline（200 行）
  - `command-server` — 命令总线（HTTP/WS 适配器）
  - `mobile-host` — 61 条命令注册表（1592 行）
  - `mobile-core` — JNI 入口（23 行）

### 1.2 共享 crates 的分支差异（字节级对比）

| Crate | 状态 | 字节差 | 文件差 | 备注 |
|---|---|---|---|---|
| `application` | **已分叉** | +5.7KB | 0 | 新增字段/函数 |
| `composition` | **已分叉** | ? | +1 文件 | 集成 `social-runtime` |
| `contracts` | **已分叉** | ? | +1 文件 | 可能新增命令契约 |
| `persistence` | **已分叉** | +1KB | 0 | 微调 |
| `platform` | **已分叉** | +5.7KB | 0 | Android 平台适配 |
| `vrchat-client` | **已分叉** | +1.2KB | 0 | 微调 |
| `application-game` | **已分叉** | -3.7KB | 0 | 可能删除桌面专属逻辑 |
| `runtime-host-desktop` | **已分叉** | -450B | 0 | 可能门控某些桌面 API |
| `host-desktop` | **已分叉** | -1.9KB | 0 | 移除 `trash` / `arboard` |
| **其他 22 个** | **完全相同** | 0 | 0 | 纯共享，无修改 |

**结论**：
- **47% 的共享 crates（15/32）已产生分支差异**
- 差异主要是"新增 Android 适配"和"移除桌面依赖"
- **22 个 crates 仍保持与主线一致**

---

## 二、跟进主线更新的三种场景

### 场景 A：主线更新「未修改的 22 个 crates」

**复杂度**：**2/10**（低）

**操作**：直接复制

```powershell
# 示例：主线更新了 application-realtime
cp -Recurse D:\vrcx0\src\crates\application-realtime D:\vrcx0\mobile\crates\
cargo build --target aarch64-linux-android  # 验证编译通过
```

**风险**：
- 新增的依赖可能不支持 Android（需检查 `Cargo.toml`）
- 破坏性 API 变更会影响 `mobile-host` 的命令实现

**时间估算**：15 分钟/crate

---

### 场景 B：主线更新「已分叉的 15 个 crates」

**复杂度**：**8/10**（高）

**操作**：三向合并

```bash
# 示例：主线更新了 composition
# 1. 查看 mobile 的修改内容
cd D:\vrcx0\mobile\crates\composition
git diff 9e9fe29ac..HEAD src/  # 假设 9e9fe29ac 是 fork 起点

# 2. 应用主线更新
git cherry-pick <主线的新提交>

# 3. 手工解决冲突
#    - mobile 新增的 social: SocialRuntime 字段
#    - 主线可能重构了 RuntimeHostState 构建器
#    - 需要人工判断哪些保留、哪些合并

# 4. 验证
cargo test --package vrcx-0-composition
```

**风险**：
| Crate | 冲突热点 | 影响范围 |
|---|---|---|
| `composition` | `RuntimeHostState` 构建器 | 所有命令实现 |
| `application` | 新增 VRChat API | `mobile-host` 的 61 条命令 |
| `platform` | Android 特化代码 | JNI 绑定 |
| `host-desktop` | `trash` / `clipboard` 门控逻辑 | 跨平台条件编译 |

**典型冲突示例**（来自 MOBILE-LOGIC.md）：

```rust
// 主线新增字段
pub struct RuntimeHostState {
    // ... 原有字段
    pub new_service: NewService,  // ← 主线新增
}

// mobile 已有自定义
pub struct RuntimeHostState {
    // ... 原有字段
    pub social: SocialRuntime,    // ← mobile 新增
}

// 合并后需要
pub struct RuntimeHostState {
    // ... 原有字段
    pub social: SocialRuntime,
    pub new_service: NewService,
}
```

**时间估算**：2-8 小时/crate（取决于冲突复杂度）

---

### 场景 C：主线新增功能，需要在 mobile 暴露命令

**复杂度**：**6/10**（中高）

**操作**：端到端移植

```rust
// 1. 主线在 application/src/remote/ 新增 VrchatFacade::new_api()
pub async fn new_api(&self, input: NewInput) -> Result<NewOutput> { ... }

// 2. mobile-host 需要注册命令
// mobile/crates/mobile-host/src/lib.rs
async fn app__vrchat_new_api(
    state: Arc<RuntimeHostState>,
    args: InputArgs<NewInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state.vrchat_remote().new_api(args.input).await?;
    serde_json::to_value(response).map_err(...)
}

// 3. 在 build_registry() 注册
registry.register_typed::<InputArgs<NewInput>, serde_json::Value, _, _>(
    "app__vrchat_new_api",
    app__vrchat_new_api,
);

// 4. Kotlin 侧封装
// vrcm-base/.../VrcxCoreClient.kt
suspend fun VrcxCoreClient.newApi(input: NewInput): VrchatResult = invoke(
    "app__vrchat_new_api",
    buildJsonObject { put("input", input.toJson()) }
).toVrchatResult()
```

**风险**：
- 主线的服务门面可能在 `runtime-host-desktop`（mobile 不能直接用）
- 需要移植到 `composition` 或新建 `mobile-host` 内的适配器

**时间估算**：30 分钟 - 2 小时/命令

---

## 三、已知的架构技术债

### 3.1 social-runtime 的来源问题

**现状**：
```
主线：crates/runtime-host-desktop/src/social.rs（一体化，14 个依赖字段）
mobile：crates/social-runtime/（独立 crate，只有 3 个依赖）
```

**来源**（MOBILE-LOGIC.md 第 27 轮）：
> 从 cloud 的 `ServerSocialRuntime` 移植：只移植了 baseline 读取（favorites_baseline / friend_roster_baseline），
> 没有移植 moderation sync、friend mutations、notification actions

**问题**：
- 主线如果重构 `runtime-host-desktop/src/social.rs` 的 baseline 逻辑，mobile 的 `social-runtime` 不会自动跟随
- 需要**手工对照**两边的差异

### 3.2 composition 的扩展点

**mobile 修改了什么**：
```rust
// mobile/crates/composition/src/state/runtime_host_state.rs
pub struct RuntimeHostState {
    // ... 主线的所有字段
    pub(crate) social: SocialRuntime,  // ← mobile 新增
}

pub fn social(&self) -> &SocialRuntime {  // ← mobile 新增
    &self.social
}
```

**风险**：
- 主线如果重构 `RuntimeHostState` 的构建方式（例如改为 Builder 模式），mobile 需要同步修改
- 当前 mobile 的构建器在 `mobile/crates/composition/src/state/runtime_host_state.rs:632`

### 3.3 platform 的 Android 适配

**mobile 新增的内容**（+5.7KB）：
- Android 的 `machine_uid` 实现（读取 `ANDROID_ID`）
- `clipboard` / `move_to_trash` 的 `unsupported` 占位

**风险**：
- 主线如果新增桌面专属的 `platform` 功能，需要在 mobile 补 `#[cfg(not(target_os = "android"))]` 门控

---

## 四、跟进流程与工具缺失

### 4.1 当前流程（手工）

```
1. 开发者在主线仓库提交更新
2. mobile 维护者定期（每周？每月？）手工同步
3. 逐个 crate 对比差异：
   - git diff src/crates/<name> mobile/crates/<name>
   - 三向合并冲突文件
   - 补全 mobile 特化逻辑
4. 编译验证：cargo build --target aarch64-linux-android
5. 运行 61 条命令的集成测试
```

**问题**：
- **无版本锁**：mobile 无法声明"我基于主线 commit X"
- **无 CI 保护**：主线合并 PR 时不会检查 mobile 兼容性
- **无自动化 diff**：需要手工逐个 crate 对比

### 4.2 缺失的工具链

| 工具 | 作用 | 当前状态 |
|---|---|---|
| Git submodule | 锁定主线 crates 的版本 | ❌ 未使用 |
| Workspace patch | 用本地路径覆盖主线依赖 | ❌ 未使用 |
| CI matrix build | 同时验证桌面 + Android | ❌ 未配置 |
| 契约测试 | 确保 61 条命令形状不变 | ❌ 只有手工 curl |
| Changelog | 记录主线→mobile 的同步历史 | ⚠️ MOBILE-LOGIC.md 部分记录 |

---

## 五、风险矩阵

| 主线变更类型 | mobile 受影响组件 | 复杂度 | 时间估算 | 失败后果 |
|---|---|---|---|---|
| **修 bug（无 API 变更）** | 无 | 1/10 | 15 分钟 | 低 |
| **新增 VRChat API 端点** | mobile-host | 3/10 | 30 分钟 | 低 |
| **重构 composition 构建器** | composition + mobile-host | 8/10 | 4-8 小时 | **高** |
| **重构 RuntimeHostState** | 所有命令实现 | 9/10 | 1-2 天 | **极高** |
| **新增 crate 依赖** | 全 workspace | 4/10 | 1-3 小时 | 中 |
| **删除废弃 API** | mobile-host 的调用点 | 5/10 | 2-4 小时 | 中 |
| **VRChat API 协议变更** | contracts + mobile-host | 6/10 | 4 小时 | 中高 |

**高风险场景示例**（来自 MOBILE-LOGIC.md）：

### 案例 1：JSON 解码器不一致（第 53-57 轮）
```
主线：DTO 在 VRChat API 响应中缺失字段 → 解码失败
mobile：CoreJson 配置为 coerceInputValues = false → 比主线更严格
结果：同一份 payload，主线能解，mobile 崩溃
修复：26、35、53 轮逐字段加默认值
```

**教训**：mobile 的 JSON 解码器配置必须与主线**严格一致**，否则会产生运行时差异

### 案例 2：social baseline 移植不完整（第 27 轮）
```
主线：DesktopSocialRuntime 包含 moderation / mutations / notifications
mobile：SocialRuntime 只包含 favorites_baseline / friend_roster_baseline
结果：mobile 无法实现通知标记已读、好友操作等功能
修复：需要补全整个 social runtime（或保持只读）
```

---

## 六、降低复杂度的改进建议

### 建议 A：版本锁定（优先级：高）

**方案 1：Git submodule**
```bash
cd D:\vrcx0\mobile
git submodule add ../src vrcx-upstream
ln -s vrcx-upstream/crates crates-upstream

# mobile/Cargo.toml
[dependencies]
vrcx-0-application = { path = "crates-upstream/application" }
# 覆盖已修改的
vrcx-0-composition = { path = "crates/composition" }
```

**方案 2：Workspace patch**
```toml
# mobile/Cargo.toml
[patch.'../src']
vrcx-0-composition = { path = "crates/composition" }
vrcx-0-social-runtime = { path = "crates/social-runtime" }
```

**优势**：
- 明确声明"mobile 基于主线 commit X"
- 主线更新时，`git submodule update` 一条命令同步所有未修改的 crates

### 建议 B：CI 矩阵构建（优先级：高）

```yaml
# .github/workflows/ci.yml
jobs:
  build:
    strategy:
      matrix:
        target:
          - x86_64-pc-windows-msvc    # 主线桌面
          - aarch64-linux-android     # mobile 分支
    steps:
      - run: cargo build --target ${{ matrix.target }}
      - run: cargo test --target ${{ matrix.target }}
```

**优势**：
- 主线每次 PR 合并都验证 mobile 兼容性
- 提前发现破坏性变更

### 建议 C：契约测试自动化（优先级：中）

```rust
// mobile/crates/mobile-host/tests/command_contracts.rs
#[test]
fn test_all_61_commands_schemas() {
    let registry = build_registry();
    for cmd in EXPECTED_COMMANDS {
        let result = registry.invoke(cmd, fixture_json(cmd));
        assert!(result.is_ok(), "{} schema mismatch", cmd);
    }
}
```

**优势**：
- 编译时检查 61 条命令的参数形状
- 避免运行时才发现契约错误

### 建议 D：分支同步 Changelog（优先级：低）

```markdown
# mobile/SYNC_LOG.md

## 2026-11-20：同步主线 @ a1b2c3d
- 更新 application（+5.7KB）：新增 VrchatFacade::new_api
- 更新 composition（+1 文件）：重构构建器，手工合并 social 字段
- 冲突解决：RuntimeHostStateBuilder::new() 参数顺序变化
- 测试：61 条命令全部通过
```

---

## 七、典型更新场景的 SOP

### SOP 1：主线发布新版本（例如 v2.5.0）

```bash
# 1. 查看主线变更
cd D:\vrcx0\src
git log v2.4.0..v2.5.0 --oneline crates/

# 2. 识别影响的 crates
git diff v2.4.0..v2.5.0 --stat crates/ | grep -E "^\s*crates/"

# 3. 对于未修改的 crates，直接复制
rsync -av D:\vrcx0\src\crates/application-realtime/ D:\vrcx0\mobile\crates/application-realtime/

# 4. 对于已修改的 crates（如 composition）
cd D:\vrcx0\mobile\crates\composition
git diff 9e9fe29ac..HEAD . > mobile-changes.patch
# 手工三向合并

# 5. 编译验证
cd D:\vrcx0\mobile
cargo build --target aarch64-linux-android

# 6. 运行集成测试
cargo test --package vrcx-0-mobile-host
# 真机验证 61 条命令

# 7. 记录同步
echo "Synced from v2.5.0 @ $(git rev-parse v2.5.0)" >> SYNC_LOG.md
```

**时间估算**：
- 小版本（bug 修复）：2-4 小时
- 中版本（新功能）：1-2 天
- 大版本（架构重构）：3-5 天

### SOP 2：主线重构核心架构（例如拆分 composition）

**风险等级**：🔴 极高

**操作步骤**：
1. **暂停 mobile 开发**（避免冲突累积）
2. **完整审计主线变更**：
   ```bash
   git diff <重构前> <重构后> crates/composition/ > refactor.diff
   ```
3. **在 mobile 分支重放变更**：
   - 优先保留 mobile 的特化逻辑（`social: SocialRuntime`）
   - 手工合并主线的新结构
4. **重新验证所有 61 条命令**：
   ```bash
   # 每条命令都要 curl 打一次真实请求
   for cmd in $(jq -r '.commands[]' commands.json); do
       curl -X POST http://localhost:8891/api/invoke \
            -d "{\"cmd\":\"$cmd\",\"args\":{}}"
   done
   ```
5. **真机回归测试**：走完所有主要页面

**时间估算**：3-7 天

---

## 八、总结：复杂度评分依据

### 评分：7/10（中高）

**扣分项**：
- **-1**：31 个共享 crates 的手工同步（无自动化）
- **-1**：15 个 crates 已分叉（冲突风险）
- **-1**：无版本锁（无法明确"基于主线哪个版本"）
- **-0.5**：缺少 CI 矩阵（主线 PR 不验证 mobile）
- **-0.5**：缺少契约测试（61 条命令靠手工验证）

**加分项**：
- **+1**：22 个 crates 保持同步（47% 无冲突）
- **+1**：mobile 独有的 4 个 crates 职责清晰（不侵入主线）
- **+0.5**：MOBILE-LOGIC.md 详细记录了所有踩坑（可追溯）
- **+0.5**：透传引擎稳定（app__web_execute 未改过）

### 与"手机侧跟进"的对比

| 维度 | 手机侧（Kotlin → Rust） | 核心侧（mobile → 主线） |
|---|---|---|
| **契约层** | 6/10（中） | **7/10（中高）** |
| **依赖版本** | 10/10（无依赖） | **3/10（无锁定）** |
| **冲突面积** | 61 条命令 | **31 个 crates** |
| **自动化** | 无（手工封装） | **无（手工合并）** |
| **回归成本** | 真机测试 | **编译 + 真机测试** |

**结论**：核心侧的复杂度**高于**手机侧，因为：
1. **冲突面积更大**（31 个 crates vs 61 条命令）
2. **影响范围更深**（破坏性 API 变更会导致编译失败）
3. **回滚成本更高**（需要重新编译整个 workspace）

---

## 九、优先级建议

### 立即实施（本周内）

1. **建立版本锁**：在 `mobile/README.md` 记录"当前基于主线 commit 9e9fe29ac"
2. **冻结 15 个已分叉 crates**：生成 `.patch` 文件，记录所有定制逻辑

### 短期（1 个月内）

3. **配置 CI 矩阵**：每次主线 PR 合并前，验证 mobile 分支编译通过
4. **编写契约测试**：61 条命令的 JSON Schema 自动验证

### 中期（3 个月内）

5. **重构 social-runtime**：与主线 `runtime-host-desktop/social.rs` 对齐
6. **统一 JSON 解码器配置**：确保 mobile 与主线的 serde 行为一致

### 长期（6 个月内）

7. **评估 workspace 合并**：是否将 mobile 作为主线的 `--target aarch64-linux-android` 构建目标
8. **生成 Kotlin Schema**：从 Rust 契约自动生成 Kotlin DTO

---

## 附录：关键文件清单

### 核心侧需要关注的文件

```
mobile/
├─ Cargo.toml                      workspace 定义（35 个成员）
├─ MOBILE-LOGIC.md                 1164 行踩坑记录
├─ MOBILE.md                       1837 行架构文档
├─ crates/
│  ├─ composition/                 与主线分叉（+1 文件）
│  │  └─ src/state/runtime_host_state.rs   集成 SocialRuntime
│  ├─ social-runtime/              mobile 独有（200 行）
│  ├─ mobile-host/                 mobile 独有（1592 行）
│  │  └─ src/lib.rs                61 条命令注册表
│  ├─ application/                 与主线分叉（+5.7KB）
│  ├─ platform/                    与主线分叉（+5.7KB，Android 适配）
│  └─ 其他 28 个                   22 个同步，6 个已分叉
```

### 手机侧需要关注的文件

```
vrcm-base/
├─ composeApp/src/commonMain/kotlin/mobilecore/
│  ├─ VrcxCoreClient.kt            1017 行命令封装
│  └─ CoreHttpEngine.kt            340 行透传引擎
└─ docs/
   ├─ ACCEPTANCE.md                11 项页面验收
   ├─ HANDOFF.md                   四处写操作步骤
   └─ SETTINGS-AUDIT.md            设置面移植审计
```

---

**报告生成时间**：2026-11-17  
**审查人**：Kiro AI  
**基于代码快照**：mobile @ 第 59 轮（冷启动验证完成）
