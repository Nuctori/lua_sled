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

### Test suite

`tests/test.lua` (~115 assertions) covers:

- **key/value**: insert/get/remove/contains_key/len/is_empty, overwrite
  returning the previous value, binary-safe keys/values (including NUL
  bytes), numeric keys with Lua `tostring` semantics (`42.0` → `"42.0"`,
  distinct from `42`)
- **iteration**: `iter`/`range` sorted order, inclusive range bounds,
  early `break`, empty-tree iteration; `scan_prefix`
- **ordered access**: `first`/`last`, `get_lt`/`get_gt` (strict),
  `pop_min`/`pop_max` (atomic)
- **trees**: namespace isolation, `tree_names`, `remove_tree` invalidating
  old handles (including `len`/`is_empty`, which the binding probes to
  avoid sled's dropped-tree infinite loop)
- **atomicity**: `compare_and_swap` success/stale, insert-if-absent
  (nil old), conditional delete (nil new), missing-argument safety;
  `transaction` commit / silent abort / Lua-error propagation
- **batches**: `apply_batch` insert+remove
- **persistence**: drop all handles (GC) then reopen — data survives;
  reopening without `create_new` works
- **input/option validation**: unknown options, non-boolean
  `create_new`/`temporary` (a stray `0` would silently enable temporary),
  `cache_capacity < 256`, non-string paths, out-of-range values

`make test` runs the suite against the release module. `make test-rust`
additionally runs the same suite from inside `cargo test`
(`tests/lua_tests.rs` loads the cdylib through the raw Lua C API), which is
what makes `make mutants` meaningful: every injected mutant is exercised
against the full Lua assertion set.

### CI (GitHub Actions)

`.github/workflows/ci.yml` runs on every push/PR:

| Job | Runner | Steps |
|-----|--------|-------|
| `linux` | ubuntu-latest | `make build` → `make test` → `make test-rust` |
| `macos` | macos-latest | same (Homebrew lua@5.4 via `PKG_CONFIG_PATH`/`LUA_BIN`) |
| `windows` | windows-latest | MSYS2 UCRT64 + GNU Rust toolchain → `mingw32-make build/test/test-rust` |
| `bench` | ubuntu-latest | `make bench` — native-vs-binding per-op ratios, fails if any exceeds `BENCH_MAX_RATIO` (200x) |

All jobs build the same source; the Windows job verifies the GNU linker
path (MinGW Lua ABI) and the bench job guards performance regressions.
`.github/workflows/release.yml` runs on `v*` tags (or manually) and
publishes prebuilt `lua_sled.so`/`.dll` artifacts for the three platforms.

## Installation

The simplest path is a source build (`make build`); with LuaRocks:

```bash
luarocks make lua-sled-scm-1.rockspec
```

Tagged GitHub releases publish prebuilt modules for Linux, macOS and
Windows (see the `release` workflow).

## Performance

`make bench` runs the same 10k-op workload through native sled and through
`lua_sled` and reports the per-op bridge cost. CI enforces a loose,
machine-independent ratio bound (`BENCH_MAX_RATIO`, default 200x) so a
performance regression fails the build.

Representative results (MSYS2/UCRT64, debug of nothing — this is a release
build; your numbers will vary with hardware):

```
--- lua_sled vs native sled (per-op) ---
insert_ns  native=3000   lua=4500   ratio=1.5x
 get_ns    native=496    lua=1500   ratio=3.0x
--- informational (lua side) ---
table_insert_ns=700      (pure Lua table, in-memory, no persistence)
table_get_ns=300
filekv_insert_ns=500     (naive file append, no fsync)
```

The mlua bridge overhead is small relative to sled's own I/O, so the binding
costs roughly 1–3x native per operation — while adding persistence, sorted
iteration, namespaces and compare-and-swap that the pure-table/file
approaches lack. `iter_ms` (full 10k scan) is printed too but not asserted.

Run it yourself: `make bench`.

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
| `db:clear()` / `db:flush()` | — |
| `db:iter()` | for-in iterator (`k, v`) |
| `db:range(start, end)` | for-in iterator (inclusive) |
| `db:scan_prefix(prefix)` | for-in iterator (keys starting with prefix) |
| `db:first()` / `db:last()` | `k, v` or nil |
| `db:get_lt(k)` / `db:get_gt(k)` | `k, v` or nil (strict) |
| `db:pop_min()` / `db:pop_max()` | `k, v` or nil (atomic pop) |
| `db:apply_batch({insert=..., remove=...})` | — |
| `db:transaction(fn)` | — (fn receives a `txn` handle) |
| `db:name()` / `db:checksum()` / `db:verify_integrity()` | string / number / — |
| `db:open_tree(name)` | `Tree` userdata |
| `db:remove_tree(name)` | boolean (was it present) |
| `db:tree_names()` | array of tree names |
| `tree:...` | the same methods as `db` |
| `db:compare_and_swap(k, old, new)` | boolean (default tree) |

Transactions:

```lua
db:transaction(function(txn)
  local cur = txn:get("counter")       -- txn:get / insert / remove
  txn:insert("counter", tostring(cur + 1))
  return true                            -- true commits; anything else aborts
end)
```

A Lua error inside the callback aborts and propagates. sled retries the
callback on conflict, so side effects may run more than once. The `txn`
handle is only valid inside the callback.

**Deadlock warning:** never touch the outer `db`/`tree` handle inside a
transaction callback. sled holds a process-wide write lock during a
transaction, and every regular method (`get`, `insert`, `iter`, `range`,
`apply_batch`, a nested `transaction`, ...) takes a non-reentrant read
lock — calling them from within the callback **permanently deadlocks the
process** (unrecoverable). Use only the `txn` handle inside the callback.

Prebuilt Windows modules link `lua54.dll`; your Lua build's ABI must match.

Notes:

- `sled` is **single-process**: a second `sled.open` on the same path while
  the first handle is alive raises a lock error. Drop all handles — Db, Tree
  and any iterator state — then `collectgarbage()` before reopening.
- Removing a tree invalidates existing handles to it: `get`/`insert`/`iter`
  raise, and `len`/`is_empty` now raise too (sled's own `len()` would hang
  forever on a dropped tree — the binding probes validity first).
- `compare_and_swap` requires all three arguments: an omitted `new` raises
  (it would otherwise be an accidental conditional delete); pass an explicit
  `nil` for insert-if-absent (`old = nil`) or conditional delete (`new = nil`).
- `cache_capacity` must be at least 256 bytes; `create_new`/`temporary` must
  be real booleans (a stray `0` or `"false"` is rejected, not coerced); the
  database path must be a string.
- Numbers convert with Lua `tostring` semantics: `42.0` → `"42.0"`, `0/0` →
  `"nan"`, `-0.0` → `"-0.0"` (so `0.0` and `-0.0` are distinct keys).
- sled 0.34 has no read-only open mode; protect the directory with file
  permissions if you need read-only access.
- sled **buffers writes**: data is durable after `flush()` (or the periodic
  flusher); a crash may lose very recent writes.
- On Windows, `lua54.dll`'s directory must be on `PATH` for the test
  binaries (an MSYS2 shell has it already).

## License

MIT. See [LICENSE](LICENSE).
