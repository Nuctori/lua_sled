# lua_sled — Lua bindings for the sled embedded key-value database.
#
# Build/test conventions follow lua-stb: a portable Makefile with `make test`
# running the Lua suite. The heavy lifting is cargo; this Makefile picks the
# right Rust toolchain per platform and exposes the module as
# `target/release/lua_sled.{dll,so}` (Lua's require name).
#
# On Windows the module links the MinGW-built system Lua, so the GNU Rust
# toolchain is required (`rustup toolchain install stable-x86_64-pc-windows-gnu`).

LUA_BIN ?= lua5.4
CARGO ?= cargo

UNAME_S := $(shell uname -s 2>/dev/null)
ifeq ($(OS),Windows_NT)
CARGO_TOOLCHAIN := +stable-x86_64-pc-windows-gnu
CARGO_ARTIFACT := target/release/lua_sled.dll
MODULE := target/release/lua_sled.dll
else ifneq (,$(findstring MINGW,$(UNAME_S)))
CARGO_TOOLCHAIN := +stable-x86_64-pc-windows-gnu
CARGO_ARTIFACT := target/release/lua_sled.dll
MODULE := target/release/lua_sled.dll
else ifneq (,$(findstring MSYS,$(UNAME_S)))
CARGO_TOOLCHAIN := +stable-x86_64-pc-windows-gnu
CARGO_ARTIFACT := target/release/lua_sled.dll
MODULE := target/release/lua_sled.dll
else ifneq (,$(findstring Darwin,$(UNAME_S)))
CARGO_TOOLCHAIN :=
CARGO_ARTIFACT := target/release/liblua_sled.dylib
MODULE := target/release/lua_sled.so
else
CARGO_TOOLCHAIN :=
CARGO_ARTIFACT := target/release/liblua_sled.so
MODULE := target/release/lua_sled.so
endif

.PHONY: all build test test-rust mutants bench clean

all: build

build: $(MODULE)

# cargo names cdylibs liblua_sled.{so,dylib} on Unix; Lua looks for
# lua_sled.so, so expose a symlink with the require name.
$(MODULE): src/lib.rs build.rs Cargo.toml
	$(CARGO) $(CARGO_TOOLCHAIN) build --release
	@if [ "$(MODULE)" != "$(CARGO_ARTIFACT)" ]; then \
	  ln -sf $(notdir $(CARGO_ARTIFACT)) $(MODULE); \
	fi

test: build
	# package.config:sub(3,3) is Lua's path separator (; on Windows, :
	# elsewhere), so this works regardless of how the Lua build was
	# configured; the default cpath stays available.
	$(LUA_BIN) -e 'local s = package.config:sub(3,3); package.cpath = "target/release/?.dll" .. s .. "target/release/?.so" .. s .. package.cpath' tests/test.lua

test-rust: build
	$(CARGO) $(CARGO_TOOLCHAIN) test

# Mutation testing: injects code mutants and re-runs the whole Lua suite
# through tests/lua_tests.rs. Surviving mutants are real coverage gaps.
# Install once with: cargo install cargo-mutants
mutants:
	$(CARGO) $(CARGO_TOOLCHAIN) mutants --file src/lib.rs --no-shuffle --jobs 1

# Performance: native sled baseline vs the Lua binding (bridge overhead),
# plus informational pure-table / file-KV comparisons. The CI-friendly
# assertion is a loose per-op ratio bound (machine-independent).
BENCH_MAX_RATIO ?= 200
bench: build
	$(CARGO) $(CARGO_TOOLCHAIN) run --release --example bench_native \
	  | grep -E '^(insert_ns|get_ns|iter_ms|count)=' > /tmp/sled_bench_native.txt
	$(LUA_BIN) -e 'local s = package.config:sub(3,3); package.cpath = "target/release/?.dll" .. s .. "target/release/?.so" .. s .. package.cpath' \
	  tests/bench.lua > /tmp/sled_bench_lua.txt
	@echo "=== lua_sled vs native sled (per-op) ==="
	@awk -F= 'FNR==NR { n[$$1]=$$2; next } $$1 ~ /_ns$$/ && $$1 in n { \
	  printf "%-12s native=%-8s lua=%-8s ratio=%.1fx\n", $$1, n[$$1], $$2, $$2/n[$$1]; \
	  if ($$2/n[$$1] > $(BENCH_MAX_RATIO)) { \
	    printf "FAIL: %s ratio %.1fx exceeds $(BENCH_MAX_RATIO)x\n", $$1, $$2/n[$$1] > "/dev/stderr"; exit 1 } }' \
	  /tmp/sled_bench_native.txt /tmp/sled_bench_lua.txt
	@echo "=== informational (lua side) ==="
	@grep -E '^(table_|filekv_)' /tmp/sled_bench_lua.txt || true
	@echo "bench ok (max per-op ratio $(BENCH_MAX_RATIO)x)"

clean:
	$(CARGO) $(CARGO_TOOLCHAIN) clean
