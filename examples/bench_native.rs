//! Native sled benchmark — the baseline that the Lua binding overhead is
//! measured against. Runs the same fixed operation set as tests/bench.lua.
//!
//! Output format is `name=value` lines consumed by `make bench`.

use std::time::Instant;

const N: u64 = 10_000;

fn main() {
    let dir = std::env::temp_dir().join(format!("sled-bench-native-{}", std::process::id()));
    let db = sled::open(&dir).expect("open native db");

    let t = Instant::now();
    for i in 0..N {
        db.insert(format!("k{i}").as_bytes(), format!("v{i}").as_bytes())
            .expect("insert");
    }
    let insert_ns = t.elapsed().as_nanos() as f64 / N as f64;

    let t = Instant::now();
    for i in 0..N {
        let _ = db.get(format!("k{i}").as_bytes()).expect("get");
    }
    let get_ns = t.elapsed().as_nanos() as f64 / N as f64;

    let t = Instant::now();
    let mut count = 0u64;
    for kv in db.iter() {
        let _ = kv.expect("iter item");
        count += 1;
    }
    let iter_ms = t.elapsed().as_millis();
    assert_eq!(count, N, "iter must see all entries");

    println!("insert_ns={insert_ns:.0}");
    println!("get_ns={get_ns:.0}");
    println!("iter_ms={iter_ms}");
    println!("count={count}");

    let _ = std::fs::remove_dir_all(&dir);
}
