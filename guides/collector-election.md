# 采集选举（collector election）使用说明

sync 数据库被多台设备共享时，**同一台 VRChat 账号的事件只允许一台设备落库**，
否则同一条好友动态会被写两遍（`feed_*` 的自然键含客户端时间戳，键约束合并不了）。
本文档说明配置方法、判定规则和验证手段。

代码位置：
`crates/composition/src/state/remote_sync/collector_election.rs`（规则 + 30s 巡检）
`crates/contracts/src/collector_gate.rs`（写入闸，进程级）
`crates/application-realtime/src/realtime/service/host/feed.rs`（非采集者不向 UI 广播 feed）

---

## 1. 配置项（本机 SQLite `configs` 表，键名前缀 `config:vrcx_remotesync.`）

| 键 | 取值 | 说明 |
|---|---|---|
| `collectorpriority` | 整数 `0-100`（默认 0） | **越大越优先采集**；留空/非法值按 0 处理 |
| `collector` | `auto`（默认）/ `off` | `off` 表示不参与选举、也不登记自己（不挡别人） |
| `intervalsec` | 秒（默认 60，最小 5） | 同步周期；同时决定选举的"活动窗口" |

桌面端在 **设置 → 数据同步 → 采集优先级** 里填数字（0-100，仅数字）；
headless 没有 UI，直接写库：

```bash
# 服务器（/opt/vrcx0/data/VRCX-0.sqlite3）
python3 - <<'PY'
import sqlite3
con = sqlite3.connect("/opt/vrcx0/data/VRCX-0.sqlite3")
for k, v in (("collectorpriority", "0"), ("collector", "auto"), ("intervalsec", "15")):
    con.execute("INSERT INTO configs(key,value) VALUES(?,?) "
                "ON CONFLICT(key) DO UPDATE SET value=?",
                ("config:vrcx_remotesync." + k, v, v))
con.commit()
PY
```

**推荐的典型布局（一台主力 PC + 一台备用 PC + 一台 24/7 服务器）**

```
主力 PC    collectorpriority = 20     ← 你在用的时候它采集，动态页零延迟
备用 PC    collectorpriority = 10     ← 主力关机后接管
服务器     collectorpriority = 0      ← 所有电脑都关机时才采集（兜底不漏数据）
三台都设 collector = auto
```

不要给两台都设成"强制"——本项目已经没有 forced 语义：**只有一个能用数字表达的顺序，
排名第一的那台才是采集者**。

---

## 1.1 只读客户端（手机 / 平板 / 网页）必须设 `collector = off`

只读端**不参与采集**，它如果按默认 `auto` 注册进去，会带来两个问题：

1. **可能抢走采集权**：优先级相同时按 device_id 字典序决出赢家，手机 UUID 恰好更小就会
   把桌面顶下去，桌面的动态立刻开始延迟；
2. **污染共享配置**：它每轮心跳都会写自己的 `configs` 行，`collectorpriority` 之类的
   本机配置会在设备间来回覆盖。

所以只读端要么设 `collector=off`（不登记自己、也不阻塞别人），要么干脆不写 `_sync_devices`。

## 1.2 健康度：能采 vs 想采

优先级只是**意愿**；能不能采是另一回事。每台设备在心跳里带一个 `health` 分数
（`_sync_devices.health`，0 或 100），来自本进程内的两个信号：

| 信号 | 健康 | 不健康 |
|---|---|---|
| VRChat WebSocket | 连着 | 近 15 分钟在线率 < 60% |
| 会话 | 有效 | `authFailure`（登录过期）→ 立即降级 |
| 同步周期 | 上一轮成功 | 连续 3 轮失败 |

规则：

- 选举**跳过** `health = 0` 的对端（它已经自己弃权，不该再压制别人）；
- 自身 `health = 0` 时**不采集**，把写入让给健康的设备；
- 恢复需要 **120 秒**连续健康（滞回），避免两台机器来回抢；
- 失败方向仍是 fail-open：未知状态按健康处理。

界面上（设置 → 数据同步 → 已连接的设备）能看到每台的 `健康度`，以及本机当前
"正在采集 / 未采集 + 原因"（`healthy` / `websocketUnstable` / `authExpired` /
`syncFailing` / `recovering` / `disabled` / `yielded`）。

> 实测案例：服务器 VRChat 会话过期导致进程崩溃重启，它的心跳照发、设备表里看起来
> 一切正常，但它根本没在采集。加上健康度后这类"僵尸设备"会被识别并让位。

## 2. 判定规则（三行就能背下来）

1. 参与节点按 **优先级降序** 排序；优先级相同则 **device_id 字典序小者赢**
   （UUID 比较，所有节点算出同一个赢家，无需协调）；
2. 只有排名第一的节点写 live feed；其余节点**照常保持 WebSocket**（好友状态、通知仍实时），
   只是不写 feed 行、也不向自己的界面广播 feed 投影——它们的事件由同步周期补进来；
3. 失败方向永远是 **fail-open（继续采集）**：判断错了最坏是多写一条，而不是静默停采。

活动窗口 = `max(45 秒, 2 × intervalsec)`：跳过一轮心跳不会被判定为离线，
避免采集权来回抖动。切换延迟最坏为 30 秒（巡检周期）+ 一个窗口。

---

## 3. 怎么确认当前谁在采集

每台设备都会把判定结果写进自己的 `_sync_meta`（**不参与同步**）：

```sql
-- 任意设备的本地库
select key, value from _sync_meta where key like 'sync.collector%';
```

```
sync.collectorRole        = auto
sync.collectorPriority    = 20
sync.collectorMayCollect  = 1              ← 1 = 本机在采集
sync.collectorBlockers    =                ← 空 = 没人压过我；有值 = 我在让位（列 device_id）
```

设备表里能看到每台广播的身份：

```sql
-- 任意设备 / 服务器
select left(device_id,8) as id, role, priority,
       extract(epoch from (now() - last_seen_at))::bigint as seen_s
from _sync_devices order by priority desc;
```

服务器侧（PostgreSQL）：

```bash
su - postgres -c "/www/server/pgsql/bin/psql -d vrcx0 -c \"
select device, count(*) from sync_ops
where table_name like '%feed%' and created_at > now() - interval '1 hour'
group by device\""
```

**期望**：只出现当前采集者那一行 device_id。若出现两行 → 选举没生效，检查
`sync.collectorPriority` 是否两边相同（相同则按 device_id 决出唯一赢家，也不该有两行）。

---

## 3.1 改了排除清单后，触发器会自动重建

`configs` 的捕获触发器把排除名单写进了 `WHEN` 子句，且建表时是
`CREATE TRIGGER IF NOT EXISTS`。但 `register_sync_table` 在每次启动扫描时是
**先 DROP 再 CREATE**，所以只要**重启一次应用**，新的排除名单就会生效 ——
不需要手工删触发器。验证办法：

```sql
-- 触发器的 WHEN 子句里应该能看到新排除的键
select name, sql from sqlite_schema where type = 'trigger' and name like '%configs%';
```

⚠️ 但如果是在**应用未运行**时直接改 SQLite（比如脚本写配置），那一刻生效的仍是旧触发器，
会被捕获一次。改完记得让应用重启一遍再写。

## 4. Schema 变化怎么手动核对（原 `sync_schema_drift` 测试已撤）

早期版本有一个 `crates/persistence/tests/sync_schema_drift.rs` 自动比对本地表与
`SYNC_TABLE_CATALOG` 的列；为了"与上游保持一致"这个测试被移除了。手动核对流程：

```powershell
# 1) 看协议里登记了哪些表、哪些键
#    crates/contracts/src/sync.rs → SYNC_TABLE_CATALOG

# 2) 上游发版后，先看本地表有没有新增/改名
python - <<'PY'
import sqlite3, os
con = sqlite3.connect(os.path.expandvars(r"%APPDATA%\VRCX-0\VRCX-0.sqlite3"))
for (t,) in con.execute("select name from sqlite_master where type='table' and name like 'usred%'"):
    print(t)
PY

# 3) 看远端物化表的列
#    su - postgres -c "psql -d vrcx0 -c '\d usred..._feed_gps'"

# 4) 对不上的表现与处理
#    - 新列缺失   → 该表的 upsert 会静默丢字段（不报错），需要给远端 ALTER TABLE
#    - 新表未登记 → 不同步（本地写、别处看不到），需要加进 SYNC_TABLE_CATALOG
#    - 列改名     → 会被当成两个字段，旧数据留在旧列
```

改完协议后记得同步改 `SYNC_PROTOCOL_SCHEMA_VERSION` 并跑一次
`cargo check --workspace --all-targets`，另外前端绑定的重新生成见
`docs/frontend-backend-ipc.md`。
