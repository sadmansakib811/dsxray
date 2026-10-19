//! The JSON form shared by the command line (`--json`) and the browser page.

use serde_json::{Value as Json, json};

use crate::{DsStore, Value, describe, parse};

/// A parsed file as JSON: summary numbers, the names it reveals, every record (with meaning,
/// readable value, raw value and byte range) and the byte map.
pub fn to_json(store: &DsStore) -> Json {
    let records: Vec<Json> = store
        .records
        .iter()
        .map(|r| {
            let d = describe(r);
            json!({
                "name": r.name,
                "code": r.code.to_string(),
                "type": r.value.type_code(),
                "value": raw(&r.value),
                "meaning": d.meaning,
                "summary": d.summary,
                "decoded": d.decoded,
                "offset": r.offset,
                "length": r.length,
            })
        })
        .collect();
    let regions: Vec<Json> = store
        .regions
        .iter()
        .map(|r| json!({ "start": r.start, "end": r.end, "kind": r.kind, "label": r.label }))
        .collect();
    json!({
        "file_size": store.file_size,
        "tree": {
            "root": store.tree.root,
            "levels": store.tree.levels,
            "records": store.tree.records,
            "nodes": store.tree.nodes,
            "page_size": store.tree.page_size,
        },
        "names": store.file_names(),
        "records": records,
        "regions": regions,
        "problem": store.problem.as_ref().map(|e| e.to_string()),
    })
}

/// Parse `data` and return `{"ok": true, "store": …}` or `{"ok": false, "error": "…"}` as text.
/// This is the whole interface of the WebAssembly build.
pub fn inspect_json(data: &[u8]) -> String {
    match parse(data) {
        Ok(store) => json!({ "ok": true, "store": to_json(&store) }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
    .to_string()
}

fn raw(v: &Value) -> Json {
    match v {
        Value::Bool(b) => json!(b),
        Value::Long(n) | Value::Shor(n) => json!(n),
        Value::Comp(n) | Value::Dutc(n) => json!(n),
        Value::Type(t) => json!(t.to_string()),
        Value::Ustr(s) => json!(s),
        Value::Blob(b) => json!(b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
    }
}
