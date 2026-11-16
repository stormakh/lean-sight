//! Shared helpers for the promlib golden tests: the plugin SDK's golden file format and `assert.JSONEq`.
#![allow(dead_code)]
use json::Json;
use std::path::PathBuf;

/// `tests-data/promlib/<rel>`.
pub fn data_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests-data/promlib").join(rel)
}

pub fn read_bytes(rel: &str) -> Vec<u8> {
    std::fs::read(data_path(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The JSON part of a `.jsonc` golden file (`readGoldenJSONFile`: the third chunk split on the machine marker).
pub fn golden(rel: &str) -> Json {
    let raw = String::from_utf8(read_bytes(rel)).expect("utf8");
    let marker = "//  🌟 This was machine generated.  Do not edit. 🌟";
    let chunks: Vec<&str> = raw.split(marker).collect();
    assert!(chunks.len() >= 3, "no golden data found in {rel}");
    Json::parse(chunks[2]).unwrap_or_else(|e| panic!("golden {rel} is not JSON: {e:?}"))
}

/// testify `assert.JSONEq`: same values, object keys in any order, numbers compared as float64.
pub fn json_eq(a: &Json, b: &Json, path: &str) -> Result<(), String> {
    match (a, b) {
        (Json::Obj(x), Json::Obj(y)) => {
            if x.len() != y.len() {
                let kx: Vec<&str> = x.iter().map(|p| p.0.as_str()).collect();
                let ky: Vec<&str> = y.iter().map(|p| p.0.as_str()).collect();
                return Err(format!("{path}: keys {kx:?} vs {ky:?}"));
            }
            for (k, v) in x.iter() {
                match y.iter().find(|p| &p.0 == k) {
                    Some((_, w)) => json_eq(v, w, &format!("{path}.{k}"))?,
                    None => return Err(format!("{path}: missing key {k}")),
                }
            }
            Ok(())
        }
        (Json::Arr(x), Json::Arr(y)) => {
            if x.len() != y.len() {
                return Err(format!("{path}: length {} vs {}", x.len(), y.len()));
            }
            for (i, (v, w)) in x.iter().zip(y.iter()).enumerate() {
                json_eq(v, w, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        (Json::Int(_) | Json::Float(_), Json::Int(_) | Json::Float(_)) => {
            let fa = a.as_f64().unwrap_or(f64::NAN);
            let fb = b.as_f64().unwrap_or(f64::NAN);
            if fa == fb { Ok(()) } else { Err(format!("{path}: {fa} vs {fb}")) }
        }
        _ => {
            if a == b {
                Ok(())
            } else {
                Err(format!("{path}: {} vs {}", a.stringify(), b.stringify()))
            }
        }
    }
}

/// `experimental.CheckGoldenJSONResponse`: the response JSON equals the golden file.
pub fn check_golden(rel: &str, dr: &grafana::promlib::frame::DataResponse) {
    let expected = golden(rel);
    let text = dr.to_json_string();
    let actual = Json::parse(&text).unwrap_or_else(|e| panic!("our JSON does not parse: {e:?}\n{text}"));
    if let Err(e) = json_eq(&expected, &actual, "$") {
        panic!("golden {rel} differs: {e}");
    }
}
