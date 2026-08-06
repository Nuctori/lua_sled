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
/// passed through (binary-safe); numbers are converted with lua_tolstring
/// semantics via coerce_string (so `42.0` becomes "42.0", matching Lua's
/// own tostring).
fn lua_bytes(lua: &Lua, value: LuaValue) -> mlua::Result<mlua::LuaString> {
    match value {
        LuaValue::String(s) => Ok(s),
        value @ (LuaValue::Number(_) | LuaValue::Integer(_)) => match lua.coerce_string(value)? {
            Some(s) => Ok(s),
            None => Err(mlua::Error::runtime(
                "lua_sled: cannot convert value to string",
            )),
        },
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
/// protocol: `for k, v in db:iter() do ... end`. sled's Iter yields None
/// permanently once exhausted, so no extra finished flag is needed.
struct LuaIter {
    iter: sled::Iter,
}

impl mlua::UserData for LuaIter {}

/// Creates a for-in triple `(next_fn, state, nil)`. The next function borrows
/// the state mutably and yields `k, v` (or nil when exhausted).
fn make_iterator(
    lua: &Lua,
    iter: sled::Iter,
) -> mlua::Result<(mlua::Function, mlua::AnyUserData, mlua::Value)> {
    let next_fn = lua.create_function(|lua, state: mlua::AnyUserData| {
        let mut iter = state.borrow_mut::<LuaIter>()?;
        match iter.iter.next() {
            Some(Ok((k, v))) => Ok(mlua::MultiValue::from_vec(vec![
                mlua::Value::String(lua.create_string(k.as_ref())?),
                mlua::Value::String(lua.create_string(v.as_ref())?),
            ])),
            Some(Err(e)) => Err(mlua::Error::runtime(format!(
                "lua_sled: iteration error: {e}"
            ))),
            None => Ok(mlua::MultiValue::from_vec(vec![mlua::Value::Nil])),
        }
    })?;
    let state = lua.create_userdata(LuaIter { iter })?;
    Ok((next_fn, state, mlua::Value::Nil))
}

/// Shared compare_and_swap implementation for Db and Tree. A nil `old` means
/// "must not exist" (insert-if-absent); a nil `new` means "delete if the
/// value matches" (sled's conditional delete).
fn cas_impl(
    lua: &Lua,
    tree: &sled::Tree,
    k: LuaValue,
    old: LuaValue,
    rest: mlua::Variadic<LuaValue>,
) -> mlua::Result<bool> {
    // Distinguish an omitted third argument (typo) from an explicit nil
    // (conditional delete): a missing argument would silently delete data.
    if rest.is_empty() {
        return Err(mlua::Error::runtime(
            "lua_sled: compare_and_swap requires key, old and new (pass nil for absent/delete)",
        ));
    }
    let new = rest[0].clone();
    let k = lua_bytes(lua, k)?;
    let old_bytes = if matches!(old, LuaValue::Nil) {
        None
    } else {
        Some(lua_bytes(lua, old)?)
    };
    let new_bytes = if matches!(new, LuaValue::Nil) {
        None
    } else {
        Some(lua_bytes(lua, new)?)
    };
    let old: Option<Vec<u8>> = old_bytes.map(|s| s.as_bytes().as_ref().to_vec());
    let new: Option<Vec<u8>> = new_bytes.map(|s| s.as_bytes().as_ref().to_vec());
    let res = tree
        .compare_and_swap(k.as_bytes().as_ref(), old, new)
        .map_err(|e| sled_err("compare_and_swap", e))?;
    Ok(res.is_ok())
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

        methods.add_method("len", |lua, this, ()| {
            // sled's Tree::len() hangs forever on a dropped tree (its
            // iterator yields Err instead of None); probe validity first.
            this.db.first().map_err(|e| sled_err("len", e))?;
            let _ = lua;
            Ok(this.db.len() as i64)
        });
        methods.add_method("is_empty", |_, this, ()| {
            this.db.first().map_err(|e| sled_err("is_empty", e))?;
            Ok(this.db.is_empty())
        });

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

        methods.add_method(
            "compare_and_swap",
            |lua, this, args: (LuaValue, LuaValue, mlua::Variadic<LuaValue>)| {
                let (k, old, rest) = args;
                // Db derefs to the default tree.
                cas_impl(lua, &this.db, k, old, rest)
            },
        );
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

        methods.add_method("len", |_, this, ()| {
            // sled's Tree::len() hangs forever on a dropped tree (its
            // iterator yields Err instead of None); probe validity first.
            this.tree.first().map_err(|e| sled_err("len", e))?;
            Ok(this.tree.len() as i64)
        });
        methods.add_method("is_empty", |_, this, ()| {
            this.tree.first().map_err(|e| sled_err("is_empty", e))?;
            Ok(this.tree.is_empty())
        });

        methods.add_method("flush", |_, this, ()| {
            this.tree.flush().map_err(|e| sled_err("flush", e))?;
            Ok(())
        });

        methods.add_method("clear", |_, this, ()| {
            this.tree.clear().map_err(|e| sled_err("clear", e))
        });

        methods.add_method(
            "compare_and_swap",
            |lua, this, args: (LuaValue, LuaValue, mlua::Variadic<LuaValue>)| {
                let (k, old, rest) = args;
                cas_impl(lua, &this.tree, k, old, rest)
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

    // sled.open(path, options?) -> Db. path must be a string (a stray number
    // would silently create a directory named e.g. "123"); booleans are
    // validated strictly (mlua would otherwise treat 0 or "false" as true).
    let open = lua.create_function(|lua, (path, opts): (LuaValue, Option<LuaTable>)| {
        let path = match &path {
            LuaValue::String(s) => s.to_string_lossy(),
            _ => {
                return Err(mlua::Error::runtime("lua_sled: path must be a string"));
            }
        };
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
            let bool_option = |opts: &LuaTable, name: &str| -> mlua::Result<Option<bool>> {
                match opts.get::<Option<LuaValue>>(name)? {
                    None => Ok(None),
                    Some(LuaValue::Boolean(b)) => Ok(Some(b)),
                    Some(_) => Err(mlua::Error::runtime(format!(
                        "lua_sled: option '{name}' must be a boolean"
                    ))),
                }
            };
            if let Some(create_new) = bool_option(&opts, "create_new")? {
                config = config.create_new(create_new);
            }
            if let Some(cap) = opts.get::<Option<u64>>("cache_capacity")? {
                if cap < 256 {
                    return Err(mlua::Error::runtime(
                        "lua_sled: cache_capacity must be at least 256 bytes",
                    ));
                }
                config = config.cache_capacity(cap);
            }
            if let Some(ms) = opts.get::<Option<u64>>("flush_every_ms")? {
                config = config.flush_every_ms(Some(ms));
            }
            if let Some(temp) = bool_option(&opts, "temporary")? {
                config = config.temporary(temp);
            }
        }
        let db = config.open().map_err(|e| sled_err("open", e))?;
        lua.create_userdata(LuaDb { db })
    })?;
    exports.set("open", open)?;
    Ok(exports)
}
