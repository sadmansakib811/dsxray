//! What the record codes mean, and readable forms of their values.
//!
//! The meanings come from the community's reverse-engineering notes (see docs/FORMAT.md) and from
//! changing Finder settings and watching which records change. Codes not listed here still parse;
//! they just have no description.

use serde_json::{Map, Value as Json, json};

use crate::{FourCc, Record, Value};

const MEANINGS: &[(&[u8; 4], &str)] = &[
    (b"BKGD", "Window background: default, a color or a picture (old format)"),
    (b"GRP0", "Group-by setting"),
    (b"ICVO", "Icon view options flag (old format)"),
    (b"Iloc", "Icon position in the folder window"),
    (b"LSVO", "List view options flag (old format)"),
    (b"bwsp", "Window size, position, toolbar and sidebar"),
    (b"cmmt", "Spotlight comment typed in Get Info"),
    (b"dilc", "Icon position on the desktop"),
    (b"dscl", "Folder is expanded in list view"),
    (b"extn", "File name extension"),
    (b"fwi0", "Window frame and view style (old format)"),
    (b"fwsw", "Sidebar width"),
    (b"fwvh", "Window height override"),
    (b"icgo", "Icon view grid offset"),
    (b"icsp", "Icon view scroll position"),
    (b"icvo", "Icon view options (old format)"),
    (b"icvp", "Icon view settings: icon size, grid, labels, background"),
    (b"icvt", "Icon label text size"),
    (b"info", "Finder info (old format)"),
    (b"lg1S", "Logical size of the folder's contents (cached)"),
    (b"logS", "Logical size of the folder's contents (cached)"),
    (b"lssp", "List view scroll position"),
    (b"lsvC", "List view columns"),
    (b"lsvP", "List view settings"),
    (b"lsvo", "List view options (old format)"),
    (b"lsvp", "List view settings: columns, sorting, text size"),
    (b"lsvt", "List view text size"),
    (b"moDD", "Modification date (cached)"),
    (b"modD", "Modification date (cached)"),
    (b"ph1S", "Size on disk of the folder's contents (cached)"),
    (b"phyS", "Size on disk of the folder's contents (cached)"),
    (b"pict", "Background picture (an alias to the image)"),
    (b"ptbL", "In the Trash: the folder this item was deleted from"),
    (b"ptbN", "In the Trash: the item's original name"),
    (b"vSrn", "Version of these settings"),
    (b"vstl", "View style the folder opens in"),
];

/// What a record means and a readable form of its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    /// What the record code stands for, if known.
    pub meaning: Option<&'static str>,
    /// A one-line readable value, e.g. `x=64, y=112` or a date.
    pub summary: String,
    /// A structured form of the value when there is one (icon position, property list, date …).
    pub decoded: Option<Json>,
}

/// Explain one record.
pub fn describe(r: &Record) -> Description {
    let meaning = MEANINGS.iter().find(|(c, _)| **c == r.code.0).map(|(_, m)| *m);
    let decoded = decode(r);
    let summary = summary(r, decoded.as_ref());
    Description { meaning, summary, decoded }
}

fn decode(r: &Record) -> Option<Json> {
    match (&r.code.0, &r.value) {
        (b"Iloc", Value::Blob(b)) if b.len() >= 8 => Some(json!({ "x": be32(b, 0), "y": be32(b, 4) })),
        (b"fwi0", Value::Blob(b)) if b.len() >= 12 => Some(json!({
            "top": be16(b, 0), "left": be16(b, 2), "bottom": be16(b, 4), "right": be16(b, 6),
            "view": view_style(&FourCc([b[8], b[9], b[10], b[11]])),
        })),
        (b"BKGD", Value::Blob(b)) if b.len() >= 4 => Some(match &b[0..4] {
            b"DefB" => json!({ "background": "default" }),
            b"ClrB" if b.len() >= 10 => json!({ "background": "color", "rgb": [be16(b, 4), be16(b, 6), be16(b, 8)] }),
            b"PctB" => json!({ "background": "picture" }),
            other => json!({ "background": String::from_utf8_lossy(other) }),
        }),
        (_, Value::Blob(b)) if b.starts_with(b"bplist") || b.starts_with(b"<?xml") => {
            plist::Value::from_reader(std::io::Cursor::new(b)).ok().map(|v| plist_json(&v))
        }
        (b"vstl", Value::Type(t)) => Some(Json::String(view_style(t))),
        (_, Value::Dutc(t)) => Some(Json::String(mac_time(*t))),
        (_, Value::Comp(n)) if matches!(&r.code.0, b"logS" | b"lg1S" | b"phyS" | b"ph1S") => {
            Some(json!({ "bytes": n, "readable": human_size(*n) }))
        }
        _ => None,
    }
}

fn summary(r: &Record, decoded: Option<&Json>) -> String {
    match (&r.value, decoded) {
        (Value::Blob(_), Some(Json::Object(m))) if r.code.0 == *b"Iloc" => {
            format!("x={}, y={}", m["x"], m["y"])
        }
        (Value::Comp(_), Some(Json::Object(m))) => m["readable"].as_str().unwrap_or_default().to_string(),
        (_, Some(Json::String(s))) => s.clone(),
        (_, Some(other)) => other.to_string(),
        (Value::Bool(b), None) => b.to_string(),
        (Value::Long(n) | Value::Shor(n), None) => n.to_string(),
        (Value::Comp(n), None) => n.to_string(),
        (Value::Dutc(n), None) => n.to_string(),
        (Value::Type(t), None) => t.to_string(),
        (Value::Ustr(s), None) => format!("{s:?}"),
        (Value::Blob(b), None) => {
            let hex: Vec<String> = b.iter().take(12).map(|x| format!("{x:02x}")).collect();
            format!("{} bytes: {}{}", b.len(), hex.join(" "), if b.len() > 12 { " …" } else { "" })
        }
    }
}

fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn view_style(t: &FourCc) -> String {
    match &t.0 {
        b"icnv" => "icon view".into(),
        b"clmv" => "column view".into(),
        b"Nlsv" => "list view".into(),
        b"Flwv" => "cover flow".into(),
        b"glyv" => "gallery view".into(),
        _ => t.to_string(),
    }
}

fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 { format!("{n} bytes") } else { format!("{v:.1} {} ({n} bytes)", UNITS[i]) }
}

/// `dutc`: 1/65536 seconds since 1904-01-01 00:00 UTC, as ISO 8601.
pub(crate) fn mac_time(t: u64) -> String {
    const MAC_TO_UNIX: i64 = 2_082_844_800;
    let secs = (t >> 16) as i64 - MAC_TO_UNIX;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn plist_json(v: &plist::Value) -> Json {
    match v {
        plist::Value::Dictionary(d) => {
            Json::Object(d.iter().map(|(k, v)| (k.clone(), plist_json(v))).collect::<Map<_, _>>())
        }
        plist::Value::Array(a) => Json::Array(a.iter().map(plist_json).collect()),
        plist::Value::Boolean(b) => Json::Bool(*b),
        plist::Value::String(s) => Json::String(s.clone()),
        plist::Value::Real(f) => serde_json::Number::from_f64(*f).map(Json::Number).unwrap_or(Json::Null),
        plist::Value::Integer(i) => match (i.as_signed(), i.as_unsigned()) {
            (Some(s), _) => json!(s),
            (None, Some(u)) => json!(u),
            _ => Json::Null,
        },
        plist::Value::Date(d) => Json::String(d.to_xml_format()),
        plist::Value::Data(b) => Json::String(format!("<{} bytes>", b.len())),
        plist::Value::Uid(u) => json!(u.get()),
        _ => Json::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_epoch_and_a_known_date() {
        assert_eq!(mac_time(0), "1904-01-01T00:00:00Z");
        // 2026-10-19 12:00:00 UTC = unix 1792411200
        assert_eq!(mac_time(((1_792_411_200i64 + 2_082_844_800) as u64) << 16), "2026-10-19T12:00:00Z");
    }

    #[test]
    fn sizes_read_like_finder() {
        assert_eq!(human_size(999), "999 bytes");
        assert_eq!(human_size(12_345), "12.3 KB (12345 bytes)");
    }
}
