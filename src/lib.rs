//! `lua_sled` — Lua bindings for the [sled](https://sled.rs) embedded
//! key-value database.
//!
//! Module mode: this cdylib links against the **host** Lua (pkg-config,
//! overridable via `LUA_LIB`/`LUA_LIB_NAME`/`LUA_LINK`), never a vendored
//! VM, so it can be `require`d from any Lua 5.4 process.
//!
//! All failures raise Lua errors prefixed with `lua_sled:`.

use mlua::prelude::*;

/// Opaque handle to an open database. `sled::Db` is an `Arc`-backed, cheaply
/// cloneable handle; the userdata owns one and drops it on GC.
struct LuaDb {
    db: sled::Db,
}

/// Opaque handle to a named tree (namespace) within a database.
struct LuaTree {
    tree: sled::Tree,
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// Converts a Lua value to a byte slice used as a sled key/value: strings are
/// passed through (binary-safe); numbers are converted via `tostring`-like
/// semantics (matching lua_tolstring).
fn lua_bytes(lua: &Lua, value: LuaValue) -> mlua::Result<mlua::LuaString> {
    match value {
        LuaValue::String(s) => Ok(s),
        LuaValue::Number(n) => lua.create_string(n.to_string()),
        LuaValue::Integer(i) => lua.create_string(i.to_string()),
        _ => Err(mlua::Error::runtime(
            "lua_sled: key/value must be a string or number",
        )),
    }
}

/// Turns a sled error into a Lua error with the lua_sled: prefix.
fn sled_err(context: &str, e: sled::Error) -> mlua::Error {
    mlua::Error::runtime(format!("lua_sled: {context}: {e}"))
}

// ---------------------------------------------------------------------------
// iteration
// ---------------------------------------------------------------------------

/// Lua iterator state: a sled scan cursor. Used via the generic for-in
/// protocol: `for k, v in db:iter() do ... end`.
struct LuaIter {
    iter: sled::Iter,
    finished: bool,
}

impl mlua::UserData for LuaIter {}

/// Creates a for-in triple `(next_fn, state, nil)`. The next function borrows
/// the state mutably and yields `k, v` (or nil when exhausted). Returning a
/// `(Vec<u8>, Vec<u8>)` tuple makes mlua push both as Lua strings.
fn make_iterator(
    lua: &Lua,
    iter: sled::Iter,
) -> mlua::Result<(mlua::Function, mlua::AnyUserData, mlua::Value)> {
    let next_fn = lua.create_function(|lua, state: mlua::AnyUserData| {
        let mut iter = state.borrow_mut::<LuaIter>()?;
        if iter.finished {
            return Ok(mlua::MultiValue::from_vec(vec![mlua::Value::Nil]));
        }
        match iter.iter.next() {
            Some(Ok((k, v))) => Ok(mlua::MultiValue::from_vec(vec![
                mlua::Value::String(lua.create_string(k.as_ref())?),
                mlua::Value::String(lua.create_string(v.as_ref())?),
            ])),
            Some(Err(e)) => Err(mlua::Error::runtime(format!(
                "lua_sled: iteration error: {e}"
            ))),
            None => {
                iter.finished = true;
                Ok(mlua::MultiValue::from_vec(vec![mlua::Value::Nil]))
            }
        }
    })?;
    let state = lua.create_userdata(LuaIter {
        iter,
        finished: false,
    })?;
    Ok((next_fn, state, mlua::Value::Nil))
}

// ---------------------------------------------------------------------------
// LuaDb
// ---------------------------------------------------------------------------

impl mlua::UserData for LuaDb {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("insert", |lua, this, (k, v): (LuaValue, LuaValue)| {
            let k = lua_bytes(lua, k)?;
            let v = lua_bytes(lua, v)?;
            let prev = this
                .db
                .insert(k.as_bytes().as_ref(), v.as_bytes().as_ref())
                .map_err(|e| sled_err("insert", e))?;
            match prev {
                Some(p) => lua.create_string(p.as_ref()).map(LuaValue::String),
                None => Ok(mlua::Value::Nil),
            }
        });

        methods.add_method("get", |lua, this, k: LuaValue| {
            let k = lua_bytes(lua, k)?;
            let v = this
                .db
                .get(k.as_bytes().as_ref())
                .map_err(|e| sled_err("get", e))?;
            match v {
                Some(p) => lua.create_string(p.as_ref()).map(LuaValue::String),
                None => Ok(mlua::Value::Nil),
            }
        });

        methods.add_method("remove", |lua, this, k: LuaValue| {
            let k = lua_bytes(lua, k)?;
            let prev = this
                .db
                .remove(k.as_bytes().as_ref())
                .map_err(|e| sled_err("remove", e))?;
            match prev {
                Some(p) => lua.create_string(p.as_ref()).map(LuaValue::String),
                None => Ok(mlua::Value::Nil),
            }
        });

        methods.add_method("contains_key", |lua, this, k: LuaValue| {
            let k = lua_bytes(lua, k)?;
            this.db
                .contains_key(k.as_bytes().as_ref())
                .map_err(|e| sled_err("contains_key", e))
        });

        methods.add_method("len", |_, this, ()| Ok(this.db.len() as i64));
        methods.add_method("is_empty", |_, this, ()| Ok(this.db.is_empty()));

        methods.add_method("flush", |_, this, ()| {
            this.db.flush().map_err(|e| sled_err("flush", e))?;
            Ok(())
        });

        methods.add_method("clear", |_, this, ()| {
            this.db.clear().map_err(|e| sled_err("clear", e))
        });

        methods.add_method("tree_names", |lua, this, ()| {
            let names = this.db.tree_names();
            let t = lua.create_table()?;
            for (i, name) in names.iter().enumerate() {
                t.set(i + 1, lua.create_string(name.as_ref())?)?;
            }
            Ok(t)
        });

        methods.add_method("open_tree", |lua, this, name: mlua::LuaString| {
            let tree = this
                .db
                .open_tree(name.as_bytes().as_ref())
                .map_err(|e| sled_err("open_tree", e))?;
            lua.create_userdata(LuaTree { tree })
        });

        methods.add_method("remove_tree", |_, this, name: mlua::LuaString| {
            let removed = this
                .db
                .drop_tree(name.as_bytes().as_ref())
                .map_err(|e| sled_err("remove_tree", e))?;
            Ok(removed)
        });

        methods.add_method("iter", |lua, this, ()| make_iterator(lua, this.db.iter()));

        methods.add_method("range", |lua, this, (start, end): (LuaValue, LuaValue)| {
            let start = lua_bytes(lua, start)?;
            let end = lua_bytes(lua, end)?;
            make_iterator(
                lua,
                this.db
                    .range(start.as_bytes().as_ref()..=end.as_bytes().as_ref()),
            )
        });
    }
}

// ---------------------------------------------------------------------------
// LuaTree
// ---------------------------------------------------------------------------

impl mlua::UserData for LuaTree {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("insert", |lua, this, (k, v): (LuaValue, LuaValue)| {
            let k = lua_bytes(lua, k)?;
            let v = lua_bytes(lua, v)?;
            let prev = this
                .tree
                .insert(k.as_bytes().as_ref(), v.as_bytes().as_ref())
                .map_err(|e| sled_err("insert", e))?;
            match prev {
                Some(p) => lua.create_string(p.as_ref()).map(LuaValue::String),
                None => Ok(mlua::Value::Nil),
            }
        });

        methods.add_method("get", |lua, this, k: LuaValue| {
            let k = lua_bytes(lua, k)?;
            let v = this
                .tree
                .get(k.as_bytes().as_ref())
                .map_err(|e| sled_err("get", e))?;
            match v {
                Some(p) => lua.create_string(p.as_ref()).map(LuaValue::String),
                None => Ok(mlua::Value::Nil),
            }
        });

        methods.add_method("remove", |lua, this, k: LuaValue| {
            let k = lua_bytes(lua, k)?;
            let prev = this
                .tree
                .remove(k.as_bytes().as_ref())
                .map_err(|e| sled_err("remove", e))?;
            match prev {
                Some(p) => lua.create_string(p.as_ref()).map(LuaValue::String),
                None => Ok(mlua::Value::Nil),
            }
        });

        methods.add_method("contains_key", |lua, this, k: LuaValue| {
            let k = lua_bytes(lua, k)?;
            this.tree
                .contains_key(k.as_bytes().as_ref())
                .map_err(|e| sled_err("contains_key", e))
        });

        methods.add_method("len", |_, this, ()| Ok(this.tree.len() as i64));
        methods.add_method("is_empty", |_, this, ()| Ok(this.tree.is_empty()));

        methods.add_method("flush", |_, this, ()| {
            this.tree.flush().map_err(|e| sled_err("flush", e))?;
            Ok(())
        });

        methods.add_method("clear", |_, this, ()| {
            this.tree.clear().map_err(|e| sled_err("clear", e))
        });

        methods.add_method(
            "compare_and_swap",
            |lua, this, (k, old, new): (LuaValue, LuaValue, LuaValue)| {
                let k = lua_bytes(lua, k)?;
                let old = lua_bytes(lua, old)?;
                let new = lua_bytes(lua, new)?;
                let res = this
                    .tree
                    .compare_and_swap(
                        k.as_bytes().as_ref(),
                        Some(old.as_bytes().as_ref()),
                        Some(new.as_bytes().as_ref()),
                    )
                    .map_err(|e| sled_err("compare_and_swap", e))?;
                Ok(res.is_ok())
            },
        );

        methods.add_method("iter", |lua, this, ()| make_iterator(lua, this.tree.iter()));

        methods.add_method("range", |lua, this, (start, end): (LuaValue, LuaValue)| {
            let start = lua_bytes(lua, start)?;
            let end = lua_bytes(lua, end)?;
            make_iterator(
                lua,
                this.tree
                    .range(start.as_bytes().as_ref()..=end.as_bytes().as_ref()),
            )
        });
    }
}

// ---------------------------------------------------------------------------
// module entry
// ---------------------------------------------------------------------------

/// Validates the options table keys for `sled.open` (strict, like lua_image).
fn reject_unknown_options(opts: &LuaTable, allowed: &[&str]) -> mlua::Result<()> {
    for pair in opts.clone().pairs::<mlua::Value, mlua::Value>() {
        let (key, _) =
            pair.map_err(|e| mlua::Error::runtime(format!("lua_sled: open options: {e}")))?;
        let key = match key {
            mlua::Value::String(s) => s.to_string_lossy().to_string(),
            _ => {
                return Err(mlua::Error::runtime(
                    "lua_sled: open option keys must be strings",
                ));
            }
        };
        if !allowed.contains(&key.as_str()) {
            return Err(mlua::Error::runtime(format!(
                "lua_sled: unknown open option '{key}'"
            )));
        }
    }
    Ok(())
}

#[mlua::lua_module]
fn lua_sled(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;

    // sled.open(path, options?) -> Db
    let open = lua.create_function(|lua, (path, opts): (String, Option<LuaTable>)| {
        let mut config = sled::Config::new().path(&path);
        if let Some(opts) = opts {
            reject_unknown_options(
                &opts,
                &[
                    "create_new",
                    "cache_capacity",
                    "flush_every_ms",
                    "temporary",
                ],
            )?;
            if let Some(create_new) = opts.get::<Option<bool>>("create_new")? {
                config = config.create_new(create_new);
            }
            if let Some(cap) = opts.get::<Option<u64>>("cache_capacity")? {
                config = config.cache_capacity(cap);
            }
            if let Some(ms) = opts.get::<Option<u64>>("flush_every_ms")? {
                config = config.flush_every_ms(Some(ms));
            }
            if let Some(temp) = opts.get::<Option<bool>>("temporary")? {
                config = config.temporary(temp);
            }
        }
        let db = config.open().map_err(|e| sled_err("open", e))?;
        lua.create_userdata(LuaDb { db })
    })?;
    exports.set("open", open)?;
    Ok(exports)
}
