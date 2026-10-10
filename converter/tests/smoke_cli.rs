use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wowmodelexport_test_{nanos}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn validate_passes_and_export_fails_for_invalid_archive_stub() {
    let temp = unique_temp_dir();
    let wow_data = temp.join("Data");
    fs::create_dir_all(&wow_data).expect("create wow data");
    fs::write(wow_data.join("patch.MPQ"), b"stub").expect("write mpq stub");
    let output = temp.join("character.glb");
    let config_path = temp.join("config.json");
    let config = serde_json::json!({
        "wow_data_path": wow_data,
        "output_path": output,
        "profile": {
            "race": "nightelf",
            "sex": "female",
            "skin_index": 1,
            "face_index": 2,
            "hair_style_index": 3,
            "hair_color_index": 4,
            "facial_hair_index": 0
        },
        "equipment_display_ids": {
            "head": 111
        },
        "animation_ids": [0, 4, 5, 69]
    });
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");

    let binary = env!("CARGO_BIN_EXE_wowmodelexport-converter");
    let validate = Command::new(binary)
        .args(["validate", "--config"])
        .arg(&config_path)
        .output()
        .expect("validate command");
    assert!(validate.status.success(), "{}", String::from_utf8_lossy(&validate.stderr));

    let export = Command::new(binary)
        .args(["export", "--config"])
        .arg(&config_path)
        .output()
        .expect("export command");
    assert!(
        !export.status.success(),
        "export unexpectedly succeeded with fake MPQ"
    );
    let stderr = String::from_utf8_lossy(&export.stderr);
    assert!(stderr.contains("open MPQ") || stderr.contains("file not found in MPQ set"));
    assert!(!output.exists(), "did not expect GLB output");
}
