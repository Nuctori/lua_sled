-- lua_sled test suite. Run with `make test` (or directly: lua5.4 tests/test.lua).

local function assert_eq(actual, expected, msg)
  if actual ~= expected then
    error((msg or "assert_eq failed") .. string.format(": expected %s, got %s",
          tostring(expected), tostring(actual)), 2)
  end
end

local function assert_error(pattern, fn)
  local ok, err = pcall(fn)
  if ok then
    error("expected error matching " .. pattern, 2)
  end
  if not tostring(err):match(pattern) then
    error(string.format("expected error matching %q, got %q", pattern, tostring(err)), 2)
  end
end

local sled = require("lua_sled")
assert_eq(type(sled), "table", "module must load")
assert_eq(type(sled.open), "function", "open must be a function")

-- Unique per-run database directory.
local dir = os.tmpname() .. "-lua_sled"
local db = sled.open(dir, { create_new = true })
assert_eq(type(db), "userdata", "db handle")

-- ---------------------------------------------------------------------------
-- basic key/value
-- ---------------------------------------------------------------------------

assert_eq(db:is_empty(), true, "fresh db is empty")
assert_eq(db:get("nope"), nil, "missing key is nil")
assert_eq(db:contains_key("nope"), false, "missing contains_key")

local prev = db:insert("k1", "v1")
assert_eq(prev, nil, "first insert returns nil")
assert_eq(db:get("k1"), "v1", "get after insert")
assert_eq(db:insert("k1", "v2"), "v1", "overwrite returns previous")
assert_eq(db:get("k1"), "v2", "overwritten value")
assert_eq(db:contains_key("k1"), true, "contains after insert")
assert_eq(db:len(), 1, "len")

-- numbers are accepted as keys/values (converted like tostring)
db:insert(42, "answer")
assert_eq(db:get(42), "answer", "numeric key")
assert_eq(db:get("42"), "answer", "numeric key normalizes to string")
db:insert("n", 7)
assert_eq(db:get("n"), "7", "numeric value converts")

-- float keys follow Lua tostring semantics (42.0 -> "42.0", not "42")
db:insert(42.0, "float-key")
assert_eq(db:get("42.0"), "float-key", "float key uses lua tostring")
assert_eq(db:get(42.0), "float-key", "float key round-trip")
assert_eq(db:get("42"), "answer", "float and integer keys are distinct")

-- NaN/Inf keys round-trip via the same literal (the exact string form is
-- implementation-defined across Lua builds, so do not assert it)
db:insert(0 / 0, "nan-val")
assert_eq(db:get(0 / 0), "nan-val", "nan key round-trips")
db:insert(1 / 0, "inf-val")
assert_eq(db:get(1 / 0), "inf-val", "inf key round-trips")

-- -0.0 and 0.0 are equal but map to different strings (Lua tostring)
db:insert(-0.0, "negzero")
assert_eq(db:get(-0.0), "negzero", "-0.0 round-trips")
assert_eq(db:get(0.0), nil, "0.0 and -0.0 are distinct keys")

-- binary-safe keys and values
local bin_key = "\0\1\2\255"
local bin_val = "\254\253\0"
db:insert(bin_key, bin_val)
assert_eq(db:get(bin_key), bin_val, "binary key/value round-trip")

-- remove
assert_eq(db:remove("n"), "7", "remove returns previous")
assert_eq(db:get("n"), nil, "removed key is nil")
assert_eq(db:remove("n"), nil, "removing missing returns nil")

-- ---------------------------------------------------------------------------
-- iteration
-- ---------------------------------------------------------------------------

db:clear()
db:insert("a", "1")
db:insert("b", "2")
db:insert("c", "3")

local iter_keys = {}
local iter_vals = {}
for k, v in db:iter() do
  iter_keys[#iter_keys + 1] = k
  iter_vals[#iter_vals + 1] = v
end
assert_eq(#iter_keys, 3, "iter yields all keys")
assert_eq(iter_keys[1], "a", "iter sorted order")
assert_eq(iter_keys[2], "b", "iter sorted order 2")
assert_eq(iter_keys[3], "c", "iter sorted order 3")
assert_eq(iter_vals[1], "1", "iter values")

-- empty db iteration terminates
db:clear()
local empty_count = 0
for _ in db:iter() do empty_count = empty_count + 1 end
assert_eq(empty_count, 0, "iter on empty db")

db:insert("a", "1")
db:insert("b", "2")
db:insert("c", "3")
db:insert("d", "4")

-- range (inclusive on both ends)
local ranged = {}
for k in db:range("b", "c") do ranged[#ranged + 1] = k end
assert_eq(#ranged, 2, "range b-c inclusive")
assert_eq(ranged[1], "b", "range start")
assert_eq(ranged[2], "c", "range end")

-- early-exit iteration (iterator is a plain for-in state, freed on break)
local first = nil
for k in db:iter() do first = k; break end
assert_eq(first, "a", "early break iteration")

-- ---------------------------------------------------------------------------
-- trees (namespaces)
-- ---------------------------------------------------------------------------

local users = db:open_tree("users")
assert_eq(type(users), "userdata", "tree handle")
users:insert("alice", "42")
users:insert("bob", "7")
assert_eq(users:get("alice"), "42", "tree get")
assert_eq(db:get("alice"), nil, "tree is isolated from the main tree")
assert_eq(users:len(), 2, "tree len")

-- trees appear in tree_names
local names = {}
for _, n in ipairs(db:tree_names()) do names[n] = true end
assert(names["users"], "tree_names contains the new tree")

-- remove_tree drops it
assert_eq(db:remove_tree("users"), true, "remove_tree returns true")
assert_eq(db:remove_tree("users"), false, "remove_tree on missing tree returns false")
-- the dropped tree handle is invalidated by sled
assert_error("does not exist", function() users:get("alice") end)
-- len/is_empty on a dropped tree must error, NOT hang (sled's iter yields
-- Err forever; the binding probes validity first)
assert_error("does not exist", function() users:len() end)
assert_error("does not exist", function() users:is_empty() end)

-- ---------------------------------------------------------------------------
-- compare_and_swap
-- ---------------------------------------------------------------------------

local counter = db:open_tree("counter")
counter:insert("n", "0")
assert_eq(counter:compare_and_swap("n", "0", "1"), true, "cas success")
assert_eq(counter:get("n"), "1", "cas applied")
assert_eq(counter:compare_and_swap("n", "stale", "99"), false, "cas stale fails")
assert_eq(counter:get("n"), "1", "cas failed leaves value")

-- cas with nil old = insert-if-absent
assert_eq(counter:compare_and_swap("absent", nil, "x"), true, "cas nil old inserts")
assert_eq(counter:get("absent"), "x", "cas insert applied")
assert_eq(counter:compare_and_swap("absent", nil, "y"), false, "cas nil old fails when present")
assert_eq(counter:get("absent"), "x", "cas insert-if-absent did not overwrite")

-- cas with nil new = conditional delete
assert_eq(counter:compare_and_swap("absent", "x", nil), true, "cas nil new deletes")
assert_eq(counter:get("absent"), nil, "cas delete applied")
assert_eq(counter:compare_and_swap("absent", "x", nil), false, "cas delete fails when absent")

-- Db also has compare_and_swap (default tree)
assert_eq(db:compare_and_swap("cas_key", nil, "v"), true, "db cas insert-if-absent")
assert_eq(db:get("cas_key"), "v", "db cas applied")

-- tree iter/range/clear/flush
local t2 = db:open_tree("t2")
t2:insert("x", "1")
t2:insert("y", "2")
local t2_count = 0
for _ in t2:iter() do t2_count = t2_count + 1 end
assert_eq(t2_count, 2, "tree iter")
local t2_range = 0
for _ in t2:range("x", "x") do t2_range = t2_range + 1 end
assert_eq(t2_range, 1, "tree range")
t2:flush()
t2:clear()
assert_eq(t2:len(), 0, "tree clear")
t2 = nil

-- ---------------------------------------------------------------------------
-- ordered access / scans / batch / transactions
-- ---------------------------------------------------------------------------

local ord = db:open_tree("ord")
ord:insert("a", "1")
ord:insert("b", "2")
ord:insert("c", "3")
ord:insert("d", "4")

local fk, fv = ord:first()
assert_eq(fk, "a", "first key")
assert_eq(fv, "1", "first value")
local lk, lv = ord:last()
assert_eq(lk, "d", "last key")
assert_eq(lv, "4", "last value")

local ltk, ltv = ord:get_lt("c")
assert_eq(ltk, "b", "get_lt key")
assert_eq(ltv, "2", "get_lt value")
local gtk, gtv = ord:get_gt("b")
assert_eq(gtk, "c", "get_gt key")
assert_eq(gtv, "3", "get_gt value")
assert_eq(ord:get_lt("a"), nil, "get_lt before first is nil")
assert_eq(ord:get_gt("d"), nil, "get_gt after last is nil")

local pmin_k, pmin_v = ord:pop_min()
assert_eq(pmin_k, "a", "pop_min key")
assert_eq(pmin_v, "1", "pop_min value")
assert_eq(ord:get("a"), nil, "pop_min removed")
local pmax_k = ord:pop_max()
assert_eq(pmax_k, "d", "pop_max key")
assert_eq(ord:len(), 2, "after pops")

ord:insert("aa", "11")
ord:insert("ab", "12")
ord:insert("ba", "21")
local prefixed = {}
for k in ord:scan_prefix("a") do prefixed[#prefixed + 1] = k end
assert_eq(#prefixed, 2, "scan_prefix finds aa, ab")
assert_eq(prefixed[1], "aa", "scan_prefix sorted")

ord:apply_batch({
  insert = { ["x"] = "X", ["y"] = "Y" },
  remove = { "b" },
})
assert_eq(ord:get("x"), "X", "batch insert")
assert_eq(ord:get("y"), "Y", "batch insert 2")
assert_eq(ord:get("b"), nil, "batch remove")

assert_eq(ord:name(), "ord", "tree name")
assert(type(ord:checksum()) == "number", "checksum is a number")
ord:verify_integrity()

local tx = db:open_tree("tx")
tx:insert("k", "0")
tx:transaction(function(t)
  local cur = t:get("k")
  t:insert("k", tostring(cur + 1))
  return true
end)
assert_eq(tx:get("k"), "1", "transaction committed")

tx:transaction(function(t)
  t:insert("k", "99")
  return false
end)
assert_eq(tx:get("k"), "1", "aborted transaction did not apply")

assert_error("boom", function()
  tx:transaction(function()
    error("boom")
  end)
end)
assert_eq(tx:get("k"), "1", "erroring transaction did not apply")

local tx2 = db:open_tree("tx2")
tx2:transaction(function(t)
  t:insert("a", "1")
  t:insert("b", "2")
  return true
end)
assert_eq(tx2:get("a"), "1", "tx2 a")
assert_eq(tx2:get("b"), "2", "tx2 b")

-- txn:insert/remove return the previous value like the regular methods
local tprev = tx2:transaction(function(t)
  local prev = t:insert("a", "10")
  t:insert("marker", prev or "nil-prev")
  return true
end)
assert_eq(tx2:get("marker"), "1", "txn insert returned previous value")

-- empty-tree ordered access
local empty = db:open_tree("empty")
assert_eq(empty:first(), nil, "empty first")
assert_eq(empty:last(), nil, "empty last")
assert_eq(empty:pop_min(), nil, "empty pop_min")
assert_eq(empty:pop_max(), nil, "empty pop_max")

-- empty batch and empty-prefix scan
empty:apply_batch({})
empty:apply_batch({ insert = {}, remove = {} })
for _ in empty:scan_prefix("") do
  assert(false, "empty prefix scan on empty tree yields nothing")
end

-- stale txn handle is safely invalidated (mlua scope destructor)
local saved_txn
local stale_ok = db:transaction(function(t)
  saved_txn = t
  return true
end)
assert_error("destructed", function() saved_txn:get("a") end)

-- dropped-tree iteration raises cleanly (lazily, on first next)
local dt = db:open_tree("dt")
dt:insert("x", "1")
db:remove_tree("dt")
assert_error("does not exist", function()
  for _ in dt:iter() do end
end)

-- db:name() is a string (the default tree id)
assert(type(db:name()) == "string", "db name is a string")

ord = nil
tx = nil
tx2 = nil
empty = nil
dt = nil
saved_txn = nil

-- ---------------------------------------------------------------------------
-- persistence: drop all handles, reopen the same directory
-- ---------------------------------------------------------------------------

db:flush()
counter = nil
users = nil
db = nil
collectgarbage("collect")

local db2 = sled.open(dir)
assert_eq(db2:get("a"), "1", "persisted value")
assert_eq(db2:get("d"), "4", "persisted d")
assert_eq(db2:open_tree("counter"):get("n"), "1", "persisted tree")
assert_eq(db2:len(), 5, "persisted len (a-d + cas_key)")

-- open options forwarding
local opt_dir = os.tmpname() .. "-opts"
local opt_db = sled.open(opt_dir, {
  create_new = true,
  cache_capacity = 1024,
  flush_every_ms = 1000,
  temporary = true,
})
assert_eq(opt_db:is_empty(), true, "options forwarded")
opt_db = nil
collectgarbage("collect")

-- ---------------------------------------------------------------------------
-- errors
-- ---------------------------------------------------------------------------

assert_error("unknown open option", function()
  sled.open(dir, { bogus = 1 })
end)
assert_error("must be a string", function() db2:insert(true, "x") end)
assert_error("must be a string", function() db2:insert("k", true) end)
assert_error("must be a string", function() db2:get({}) end)
assert_error("must be a string", function() db2:insert("k") end)
assert_error("must be a string", function() sled.open() end)
assert_error("path must be a string", function() sled.open(123) end)
assert_error("must be a boolean", function()
  sled.open(os.tmpname() .. "-b", { create_new = "yes" })
end)
assert_error("must be a boolean", function()
  sled.open(os.tmpname() .. "-b", { temporary = 0 })
end)
assert_error("cache_capacity must be at least 256", function()
  sled.open(os.tmpname() .. "-b", { cache_capacity = 100 })
end)
-- compare_and_swap without the new argument must error (not delete!)
assert_error("requires key, old and new", function()
  db2:compare_and_swap("cas_key", "v")
end)
-- ...and the value must still be there
assert_eq(db2:get("cas_key"), "v", "cas missing-arg did not delete")

-- ---------------------------------------------------------------------------
-- reopen without create_new on an existing dir (after dropping handles)
-- ---------------------------------------------------------------------------

db2 = nil
collectgarbage("collect")
local db3 = sled.open(dir)
assert_eq(db3:get("a"), "1", "reopen without options")

db3 = nil
collectgarbage("collect")
print("lua_sled tests passed")
