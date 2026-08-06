fn main() {
    // On macOS, a Lua module's Lua API symbols are resolved at load time
    // from the host process, so the cdylib must allow undefined symbols.
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-cdylib-link-arg=-undefined");
        println!("cargo:rustc-cdylib-link-arg=dynamic_lookup");
    }

    // tests/lua_tests.rs calls Lua C API symbols directly through mlua::ffi.
    // The cdylib defers them to the host, but the test *executable* must
    // resolve them at link time, so link the system Lua only for test
    // targets (rustc-link-arg-tests).
    if let Ok(lib) = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("lua5.4")
    {
        for path in &lib.link_paths {
            println!("cargo:rustc-link-arg-tests=-L{}", path.display());
        }
        for name in &lib.libs {
            println!("cargo:rustc-link-arg-tests=-l{name}");
        }
    }
}
