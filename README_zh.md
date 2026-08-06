# lua_sled

> English · **[中文文档](README_zh.md)**

`lua_sled` 是一个用 Rust 编写的 Lua C 模块，绑定
[sled](https://sled.rs)——纯 Rust 实现的嵌入式键值数据库（支持 ACID 事务）。

## 特性

- `sled.open(path, options?)` — 打开/创建数据库
  （`create_new`、`cache_capacity`、`flush_every_ms`、`temporary`）
- **键值**：`insert`（返回旧值）、`get`、`remove`、`contains_key`、`len`、
  `is_empty`、`clear`、`flush` — 键值均为二进制安全字符串（数字按
  `tostring` 转换）
- **迭代**：`iter()` 与 `range(start, end)` 支持 Lua 泛型
  `for k, v in ... do`，按键排序输出
- **树（命名空间）**：`open_tree`、`remove_tree`、`tree_names`；树是数据库
  内的隔离集合
- **原子操作**：`compare_and_swap(key, old, new)` 无锁更新；`old` 传 nil
  表示不存在则插入，`new` 传 nil 表示条件删除
- 模块抛出的错误带 `lua_sled:` 前缀（参数/类型错误来自 Lua 自身的转换）

## 依赖要求

- Rust（stable）— Windows 还需 `x86_64-pc-windows-gnu` 工具链
- Lua 5.4（开发头文件 + `pkg-config`）

## 构建与测试

```bash
make build       # cargo build --release + 模块符号链接
make test        # 运行 tests/test.lua
make test-rust   # cargo test：Rust 单测 + 完整 Lua 套件
make mutants     # 变异测试（需 `cargo install cargo-mutants`）
make bench       # 性能对比：lua_sled vs 原生 sled（+ CI 比值守卫）
```

## 安装

最简单的方式是源码构建（`make build`）；或使用 LuaRocks：

```bash
luarocks make lua-sled-scm-1.rockspec
```

打 tag 的 GitHub 发布会自动产出 Linux、macOS、Windows 预编译模块
（见 `release` workflow）。

## 性能

`make bench` 用同一 10k 操作工作负载分别跑原生 sled 与 `lua_sled`，报告
每操作的桥接成本。CI 强制宽松且机器无关的比值上限
（`BENCH_MAX_RATIO`，默认 200x），性能回退会令构建失败。

代表性数据（MSYS2/UCRT64 release 构建；具体数值随硬件变化）：

```
--- lua_sled vs 原生 sled（每操作） ---
insert_ns  native=3000   lua=4500   ratio=1.5x
 get_ns    native=496    lua=1500   ratio=3.0x
--- 信息性对比（Lua 侧） ---
table_insert_ns=700      （纯 Lua table，内存，无持久化）
table_get_ns=300
filekv_insert_ns=500     （朴素文件追加，无 fsync）
```

mlua 桥接开销相对 sled 自身的 I/O 成本很小，绑定约为原生每操作 1–3 倍
——同时获得了纯 table/文件方案所没有的持久化、排序迭代、命名空间与
compare-and-swap。`iter_ms`（10k 全量扫描）也会打印但不参与断言。

自行运行：`make bench`。

## 使用示例

```lua
local sled = require "lua_sled"

-- 打开（或创建）数据库
local db = sled.open("myapp.sled", { create_new = true })

-- 二进制安全的键值
db:insert("name", "pi")
db:insert("count", "1")
print(db:get("name"))          -- "pi"
print(db:get("missing"))       -- nil

-- 按键序迭代（泛型 for）
for k, v in db:iter() do
  print(k, v)
end

-- 范围查询
for k, v in db:range("a", "m") do end

-- 命名空间
local users = db:open_tree("users")
users:insert("alice", "42")

-- 无锁更新
local ok = db:open_tree("counter"):compare_and_swap("n", "1", "2")

-- 持久化：sled 缓冲写入，flush() 强制落盘
db:flush()
```

## API

| 函数 | 返回 |
|------|------|
| `sled.open(path, options?)` | `Db` userdata |
| `db:insert(k, v)` | 旧值或 nil |
| `db:get(k)` | 值或 nil |
| `db:remove(k)` | 旧值或 nil |
| `db:contains_key(k)` | 布尔 |
| `db:len()` / `db:is_empty()` | 整数 / 布尔 |
| `db:clear()` / `db:flush()` | — |
| `db:iter()` | for-in 迭代器（`k, v`） |
| `db:range(start, end)` | for-in 迭代器（闭区间） |
| `db:scan_prefix(prefix)` | for-in 迭代器（前缀匹配） |
| `db:first()` / `db:last()` | `k, v` 或 nil |
| `db:get_lt(k)` / `db:get_gt(k)` | `k, v` 或 nil（严格） |
| `db:pop_min()` / `db:pop_max()` | `k, v` 或 nil（原子弹出） |
| `db:apply_batch({insert=..., remove=...})` | — |
| `db:transaction(fn)` | —（fn 接收 `txn` 句柄） |
| `db:name()` / `db:checksum()` / `db:verify_integrity()` | 字符串 / 数字 / — |
| `db:flush()` | — |
| `db:iter()` | for-in 迭代器（`k, v`） |
| `db:range(start, end)` | for-in 迭代器（闭区间） |
| `db:open_tree(name)` | `Tree` userdata |
| `db:remove_tree(name)` | 布尔（是否存在） |
| `db:tree_names()` | 树名数组 |
| `tree:...` | 与 `db` 相同的键值/迭代方法 |
| `tree:compare_and_swap(k, old, new)` | 布尔 |
| `db:compare_and_swap(k, old, new)` | 布尔（默认树） |

事务示例：

```lua
db:transaction(function(txn)
  local cur = txn:get("counter")       -- txn:get / insert / remove
  txn:insert("counter", tostring(cur + 1))
  return true                            -- true 提交；其他值放弃
end)
```

回调内的 Lua 错误会放弃事务并向上传播。sled 在冲突时会重试回调，因此
副作用可能执行多次。`txn` 句柄只在回调内有效。

**死锁警告**：事务回调内**不要**使用外层 `db`/`tree` 句柄。sled 在事务
期间持有进程级写锁，而所有常规方法（`get`、`insert`、`iter`、`range`、
`apply_batch`、嵌套 `transaction`……）都获取不可重入的读锁——在回调内
调用它们会**永久挂死进程**（不可恢复）。回调内只使用 `txn` 句柄。

Windows 预编译模块链接 `lua54.dll`；你的 Lua 构建 ABI 必须匹配。

注意事项：

- **sled 单进程**：同一路径在句柄存活期间二次 `sled.open` 会报锁错误。
  重开前请释放所有句柄（Db、Tree 以及迭代器状态），再 `collectgarbage()`。
- 删除树会使指向它的旧句柄失效：`get`/`insert`/`iter` 报错，`len`/
  `is_empty` 也报错（sled 自身的 `len()` 在已删树上会**永久挂死**——绑定
  先探测有效性）。
- `compare_and_swap` 要求三个参数：省略 `new` 会报错（否则会成为一次
  意外的条件删除）；显式传 `nil` 表示不存在则插入（`old = nil`）或条件
  删除（`new = nil`）。
- `cache_capacity` 必须至少 256 字节；`create_new`/`temporary` 必须是真
  布尔（`0` 或 `"false"` 会被拒绝而非静默转换）；数据库路径必须是字符串。
- 数字按键 `tostring` 语义转换：`42.0` → `"42.0"`、`0/0` → `"nan"`、
  `-0.0` → `"-0.0"`（因此 `0.0` 与 `-0.0` 是不同键）。
- sled 0.34 无只读打开模式；需要只读时请用文件系统权限保护目录。
- **sled 缓冲写入**：数据在 `flush()`（或周期刷盘）后才保证落盘；崩溃
  可能丢失最近的写入。
- Windows 上运行测试二进制需要 `lua54.dll` 所在目录在 `PATH`（MSYS2
  shell 已包含）。

## 许可证

MIT，见 [LICENSE](LICENSE)。
