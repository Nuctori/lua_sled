# Changelog

## [Unreleased]

### Added
- High-value sled APIs: `transaction` (Lua-callback ACID transactions with
  `txn:insert/get/remove`, commit on `true`, silent abort otherwise, Lua
  errors propagate), `apply_batch` (`{ insert = {k=v,...}, remove = {k,...} }`),
  `first`/`last`, `pop_min`/`pop_max` (atomic pop), `get_lt`/`get_gt`,
  `scan_prefix` (for-in iterator), `name`, `checksum`, `verify_integrity` —
  exposed on both `Db` and `Tree` (sled's methods are macro-generated over a
  shared `TreeAccess`).
- Release layer: `lua-sled-scm-1.rockspec`, tag-triggered `release` workflow
  publishing Linux/macOS/Windows prebuilt modules, CHANGELOG.

### Fixed (adversarial audit)
- `len()`/`is_empty()` on a dropped tree hung forever (sled's iterator
  yields Err instead of None); the binding probes validity first and raises.
- `compare_and_swap` with an omitted `new` silently deleted data; a missing
  argument now raises (an explicit `nil` still means conditional delete).
- `cache_capacity < 256` rejected (sled panics below that).
- `create_new`/`temporary` must be real booleans (a stray `0`/`"false"` was
  coerced to true — `temporary = 0` would delete the data directory).
- `sled.open` path must be a string (a stray number created a directory
  named e.g. "123").
- Float keys use Lua `tostring` semantics (`42.0` -> `"42.0"`, not `"42"`).

## 0.1.0 — 2026-08-06

Initial release: `sled.open` with strict options, binary-safe KV
(insert/get/remove/contains_key/len/is_empty/clear/flush), sorted iteration
(`iter`/`range` as Lua generic for), namespaces (`open_tree`/`remove_tree`/
`tree_names`), `compare_and_swap`. Test suite (~75 assertions) runs under
`cargo test` for mutation testing; CI on Linux/macOS/Windows plus a
native-vs-binding benchmark guard.
