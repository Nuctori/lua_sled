# lua_sled

> **[中文文档](README_zh.md)** · English

`lua_sled` is a Lua C module (built with Rust) binding
[sled](https://sled.rs) — a pure-Rust embedded key-value database with
ACID transactions, for Lua.

It follows the lua-stb engineering standard used by
[lua_image](https://github.com/Nuctori/lua_image): module mode links the
**host** Lua ABI (never a vendored VM), tests run under `cargo test` so
mutation testing covers the full Lua assertion suite, and CI spans
Linux / macOS / Windows.

## Features

- `sled.open(path, options?)` — open/create a database
  (`create_new`, `cache_capacity`, `flush_every_ms`, `temporary`)
- **key/value**: `insert` (returns previous), `get`, `remove`, `contains_key`,
  `len`, `is_empty`, `clear`, `flush` — keys/values are binary-safe strings
  (numbers convert like `tostring`)
- **iteration**: `iter()` and `range(start, end)` work with Lua's generic
  `for k, v in ... do` and yield keys in sorted order
- **trees (namespaces)**: `open_tree`, `remove_tree`, `tree_names`;
  trees are isolated collections within one database
- **atomicity**: `compare_and_swap(key, old, new)` for lock-free updates; a
  nil `old` means insert-if-absent, a nil `new` means conditional delete
- errors raised by the module carry a `lua_sled:` prefix (argument/type
  errors come from Lua's own conversions)

## Requirements

- Rust (stable) — Windows additionally needs the `x86_64-pc-windows-gnu`
  toolchain
- Lua 5.4 (dev headers + `pkg-config`)

## Build & Test

```bash
make build       # cargo build --release + module symlink
make test        # run tests/test.lua
make test-rust   # cargo test: Rust unit tests + the full Lua suite
make mutants     # mutation testing (needs `cargo install cargo-mutants`)
make bench       # performance: lua_sled vs native sled (+ CI ratio guard)
```

## Performance

`make bench` runs the same 10k-op workload through native sled and through
`lua_sled` and reports the per-op bridge cost (CI enforces a loose ratio
bound so performance regressions fail the build):

```
insert_ns native=3310 lua=3900 ratio=1.2x
get_ns    native=518  lua=1100 ratio=2.1x
```

The mlua bridge overhead is small relative to sled's own I/O, so the binding
costs roughly 1–2x native per operation.

## Usage

```lua
local sled = require "lua_sled"

-- open (or create) a database
local db = sled.open("myapp.sled", { create_new = true })

-- key/value with binary-safe strings
db:insert("name", "pi")
db:insert("count", "1")
print(db:get("name"))          -- "pi"
print(db:get("missing"))       -- nil

-- iterate in sorted order (generic for)
for k, v in db:iter() do
  print(k, v)
end

-- range queries
for k, v in db:range("a", "m") do end

-- namespaces
local users = db:open_tree("users")
users:insert("alice", "42")

-- lock-free update
local ok = db:open_tree("counter"):compare_and_swap("n", "1", "2")

-- persistence: sled buffers writes; flush() forces durability
db:flush()
```

## API

| Function | Returns |
|----------|---------|
| `sled.open(path, options?)` | `Db` userdata |
| `db:insert(k, v)` | previous value or nil |
| `db:get(k)` | value or nil |
| `db:remove(k)` | previous value or nil |
| `db:contains_key(k)` | boolean |
| `db:len()` / `db:is_empty()` | integer / boolean |
| `db:clear()` | — |
| `db:flush()` | — |
| `db:iter()` | for-in iterator (`k, v`) |
| `db:range(start, end)` | for-in iterator (inclusive) |
| `db:open_tree(name)` | `Tree` userdata |
| `db:remove_tree(name)` | boolean (was it present) |
| `db:tree_names()` | array of tree names |
| `tree:...` | same KV/iteration methods as `db` |
| `tree:compare_and_swap(k, old, new)` | boolean |
| `db:compare_and_swap(k, old, new)` | boolean (default tree) |

Notes:

- `sled` is **single-process**: a second `sled.open` on the same path while
  the first handle is alive raises a lock error. Drop all handles — Db, Tree
  and any iterator state — then `collectgarbage()` before reopening.
- Removing a tree invalidates existing handles to it (sled semantics); the
  default tree cannot be removed.
- sled 0.34 has no read-only open mode; protect the directory with file
  permissions if you need read-only access.
- sled **buffers writes**: data is durable after `flush()` (or the periodic
  flusher); a crash may lose very recent writes.
- On Windows, `lua54.dll`'s directory must be on `PATH` for the test
  binaries (an MSYS2 shell has it already).

## License

MIT. See [LICENSE](LICENSE).
