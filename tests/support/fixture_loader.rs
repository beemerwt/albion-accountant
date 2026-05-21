use std::{fs, path::PathBuf};

pub fn load_hex_fixture(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    let raw = fs::read_to_string(path).expect("hex fixture readable");
    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.len() % 2 == 0, "hex fixture must have even length");

    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("utf8 hex"), 16).expect("hex byte")
        })
        .collect()
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
