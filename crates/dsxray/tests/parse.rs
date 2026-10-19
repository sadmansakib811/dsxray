//! The fixtures were written by an independent implementation (the Python `ds_store` library) or,
//! for the multi-level tree, packed by hand and checked with that library's reader
//! (scripts/make_fixtures.py). The same parser also matched ds_store on every one of 1,962 records
//! in 79 real Finder files (scripts/differential.py), which stay private.

use dsxray::{Error, Value, describe, inspect_json, parse};

const SMALL: &[u8] = include_bytes!("fixtures/small.DS_Store");
const BIG: &[u8] = include_bytes!("fixtures/big.DS_Store");

#[test]
fn reads_every_record_of_a_small_file() {
    let s = parse(SMALL).unwrap();
    assert_eq!(s.problem, None);
    assert_eq!(s.records.len(), 16);
    assert_eq!(s.tree.records, 16);
    assert_eq!(s.file_names(), ["Résumé – final.pdf", "holiday.jpg", "notes.txt", "📷 camera roll"]);

    let find = |name: &str, code: &[u8; 4]| s.records.iter().find(|r| r.name == name && r.code.0 == *code).unwrap();
    assert_eq!(describe(find("holiday.jpg", b"Iloc")).summary, "x=64, y=112");
    assert_eq!(find("notes.txt", b"cmmt").value, Value::Ustr("remember to back this up".into()));
    assert_eq!(find(".", b"vstl").value, Value::Type(dsxray::FourCc(*b"icnv")));
    assert_eq!(describe(find(".", b"vstl")).summary, "icon view");
    assert_eq!(find(".", b"lsvt").value, Value::Shor(12));
    assert_eq!(find("📷 camera roll", b"dscl").value, Value::Bool(true));
    assert_eq!(describe(find("📷 camera roll", b"modD")).summary, "2026-10-19T12:00:00Z");
    assert_eq!(describe(find("📷 camera roll", b"logS")).summary, "1.2 GB (1234567890 bytes)");

    let window = describe(find(".", b"bwsp")).decoded.unwrap();
    assert_eq!(window["WindowBounds"], "{{120, 140}, {760, 480}}");
    assert_eq!(window["ShowSidebar"], true);
    let frame = describe(find(".", b"fwi0")).decoded.unwrap();
    assert_eq!(
        (frame["top"].as_u64(), frame["right"].as_u64(), frame["view"].as_str()),
        (Some(140), Some(880), Some("icon view"))
    );
    let bg = describe(find(".", b"BKGD")).decoded.unwrap();
    assert_eq!(bg["rgb"], serde_json::json!([65535, 57344, 49152]));
}

#[test]
fn walks_a_tree_with_branch_nodes_in_order() {
    let s = parse(BIG).unwrap();
    assert_eq!(s.problem, None);
    assert_eq!(s.records.len(), 300);
    assert_eq!(s.tree.levels, 1);
    assert_eq!(s.regions.iter().filter(|r| r.kind == "leaf").count(), 8);
    assert_eq!(s.regions.iter().filter(|r| r.kind == "branch").count(), 1);
    let names: Vec<&str> = s.records.iter().map(|r| r.name.as_str()).collect();
    let expected: Vec<String> = (0..300).map(|i| format!("photo-{i:04}.jpg")).collect();
    assert_eq!(names, expected);
}

#[test]
fn record_byte_ranges_point_at_the_record() {
    for data in [SMALL, BIG] {
        let s = parse(data).unwrap();
        for r in &s.records {
            let units = u32::from_be_bytes(data[r.offset..r.offset + 4].try_into().unwrap()) as usize;
            assert_eq!(units, r.name.encode_utf16().count());
            let code_at = r.offset + 4 + 2 * units;
            assert_eq!(&data[code_at..code_at + 4], &r.code.0);
            assert!(r.offset + r.length <= data.len());
        }
    }
}

#[test]
fn regions_dont_overlap() {
    for data in [SMALL, BIG] {
        let s = parse(data).unwrap();
        for w in s.regions.windows(2) {
            assert!(w[0].end <= w[1].start, "{:?} overlaps {:?}", w[0], w[1]);
        }
    }
}

#[test]
fn rejects_what_isnt_a_ds_store() {
    assert_eq!(parse(b""), Err(Error::NotDsStore));
    assert_eq!(parse(b"\0\0\0\x01Bud2 and then some more bytes to pass the length check"), Err(Error::NotDsStore));
    let mut copy = SMALL.to_vec();
    copy[16] ^= 0xff; // the second copy of the allocator's offset
    assert!(matches!(parse(&copy), Err(Error::Corrupt(_))));
}

#[test]
fn a_node_that_points_back_at_itself_is_caught() {
    let s = parse(BIG).unwrap();
    let branch = s.regions.iter().find(|r| r.kind == "branch").unwrap();
    let mut copy = BIG.to_vec();
    copy[branch.start + 8..branch.start + 12].copy_from_slice(&s.tree.root.to_be_bytes()); // first child = itself
    let looped = parse(&copy).unwrap();
    assert_eq!(looped.problem, Some(Error::NodeLoop(s.tree.root)));
}

#[test]
fn every_truncation_fails_cleanly() {
    for data in [SMALL, BIG] {
        for len in 0..data.len() {
            let _ = parse(&data[..len]); // must not panic, hang or allocate wildly
        }
    }
}

#[test]
fn random_damage_never_panics() {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for round in 0..20_000 {
        let mut data = if round % 2 == 0 { SMALL.to_vec() } else { BIG.to_vec() };
        for _ in 0..1 + next() % 8 {
            let at = (next() % data.len() as u64) as usize;
            data[at] = next() as u8;
        }
        let _ = parse(&data);
    }
}

#[test]
fn json_answer_for_the_browser() {
    let ok: serde_json::Value = serde_json::from_str(&inspect_json(SMALL)).unwrap();
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["store"]["names"].as_array().unwrap().len(), 4);
    assert_eq!(ok["store"]["records"][0]["code"], "BKGD");
    let bad: serde_json::Value = serde_json::from_str(&inspect_json(b"not a ds_store")).unwrap();
    assert_eq!(bad["ok"], false);
}
