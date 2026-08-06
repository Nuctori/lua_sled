-- lua_sled benchmark: same fixed operation set as examples/bench_native.rs,
-- plus informational comparisons against pure-Lua tables (memory) and a
-- naive file-backed KV (simplest persistence).
--
-- Output format is `name=value` lines consumed by `make bench`.

local sled = require "lua_sled"
local N = 10000

local dir = os.tmpname() .. "-sled-bench"
local db = sled.open(dir, { create_new = true })

local function bench_ops(label, fn)
  local t0 = os.clock()
  fn()
  local ns_per_op = (os.clock() - t0) * 1e9 / N
  print(label .. "=" .. string.format("%.0f", ns_per_op))
end

bench_ops("insert_ns", function()
  for i = 1, N do
    db:insert("k" .. i, "v" .. i)
  end
end)

bench_ops("get_ns", function()
  for i = 1, N do
    db:get("k" .. i)
  end
end)

-- total iteration time over N entries
local t0 = os.clock()
local count = 0
for _ in db:iter() do
  count = count + 1
end
assert(count == N, "iter must see all entries")
print("iter_ms=" .. string.format("%.0f", (os.clock() - t0) * 1e3))
print("count=" .. count)

-- informational: pure-Lua table (in-memory, no persistence)
local tbl = {}
bench_ops("table_insert_ns", function()
  for i = 1, N do
    tbl["k" .. i] = "v" .. i
  end
end)
bench_ops("table_get_ns", function()
  for i = 1, N do
    local _ = tbl["k" .. i]
  end
end)

-- informational: naive file-backed KV (one line per entry, fsync on close)
local f = io.open(dir .. "-filekv", "wb")
bench_ops("filekv_insert_ns", function()
  for i = 1, N do
    f:write("k", i, "=", "v", i, "\n")
  end
end)
f:close()

db = nil
collectgarbage("collect")
