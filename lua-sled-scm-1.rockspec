package = "lua-sled"
version = "scm-1"
source = {
  url = "git+https://github.com/Nuctori/lua_sled",
}
description = {
  summary = "Lua bindings for the sled embedded key-value database",
  detailed = "Lua C extension exposing sled's key/value store: binary-safe " ..
             "KV ops, sorted iteration, ranges, prefix scans, namespaces, " ..
             "compare-and-swap, batched writes and ACID transactions.",
  license = "MIT",
  homepage = "https://github.com/Nuctori/lua_sled",
}
dependencies = {
  "lua >= 5.4",
}
build = {
  type = "make",
  build_target = "build",
  install_target = "install",
  build_variables = {
    LUA_VERSION = "$(LUA_VERSION)",
    PREFIX = "$(PREFIX)",
  },
}
