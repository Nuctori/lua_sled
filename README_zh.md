# lua_sled

> English · **[中文文档](README_zh.md)**

`lua_sled` 是一个用 Rust 编写的 Lua C 模块，绑定
[sled](https://sled.rs)——纯 Rust 实现的嵌入式键值数据库（支持 ACID 事务）。

遵循 [lua_image](https://github.com/Nuctori/lua_image) 采用的 lua-stb 工程
标准：module mode 链接**宿主 Lua ABI**（绝无双 VM 问题）、测试跑在
`cargo test` 下（变异测试覆盖完整 Lua 断言套件）、CI 覆盖
Linux / macOS / Windows。

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
- **原子操作**：`compare_and_swap(key, old, new)` 无锁更新
- 所有失败抛出带 `lua_sled:` 前缀的 Lua 错误

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

## 性能

`make bench` 用同一 10k 操作工作负载分别跑原生 sled 与 `lua_sled`，报告
每操作的桥接成本（CI 强制宽松比值上限，性能回退会令构建失败）：

```
insert_ns native=3310 lua=3900 ratio=1.2x
get_ns    native=518  lua=1100 ratio=2.1x
```

mlua 桥接开销相对 sled 自身的 I/O 成本很小，绑定约为原生每操作 1–2 倍。

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

-- 持久化自动进行（写入即落盘）；flush 强制同步
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
| `db:clear()` | — |
| `db:flush()` | — |
| `db:iter()` | for-in 迭代器（`k, v`） |
| `db:range(start, end)` | for-in 迭代器（闭区间） |
| `db:open_tree(name)` | `Tree` userdata |
| `db:remove_tree(name)` | 布尔（是否存在） |
| `db:tree_names()` | 树名数组 |
| `tree:...` | 与 `db` 相同的键值/迭代方法 |
| `tree:compare_and_swap(k, old, new)` | 布尔 |

注意事项：

- **sled 单进程**：同一路径在句柄存活期间二次 `sled.open` 会报锁错误。
  重开前请释放所有句柄（赋 nil + `collectgarbage()`）。
- 删除树会使指向它的旧句柄失效（sled 语义）。
- sled 0.34 无只读打开模式；需要只读时请用文件系统权限保护目录。

## 许可证

MIT，见 [LICENSE](LICENSE)。
