use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wowmodelexport_real_{nanos}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn exports_glb_with_real_client_data_when_enabled() {
    let Ok(wow_data) = std::env::var("WME_WOW_DATA") else {
        eprintln!("skipping: WME_WOW_DATA is not set");
        return;
    };
    let wow_data_path = PathBuf::from(&wow_data);
    if !wow_data_path.exists() {
        eprintln!("skipping: WME_WOW_DATA does not exist: {}", wow_data);
        return;
    }

    let temp = unique_temp_dir();
    let output = temp.join("character.glb");
    let config_path = temp.join("config.json");
    let config = serde_json::json!({
        "wow_data_path": wow_data_path,
        "output_path": output,
        "profile": {
            "race": "human",
            "sex": "female",
            "skin_index": 0,
            "face_index": 0,
            "hair_style_index": 0,
            "hair_color_index": 0,
            "facial_hair_index": 0
        },
        "equipment_display_ids": {
            "head": 0,
            "shoulder": 0,
            "chest": 0,
            "hands": 0,
            "mainhand": 0,
            "offhand": 0
        },
        "animation_ids": [0, 4, 5]
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
    assert!(
        validate.status.success(),
        "validate failed: {}",
        String::from_utf8_lossy(&validate.stderr)
    );

    let export = Command::new(binary)
        .args(["export", "--config"])
        .arg(&config_path)
        .output()
        .expect("export command");
    assert!(
        export.status.success(),
        "export failed: {}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert!(output.exists(), "expected GLB output");
    let bytes = fs::read(&output).expect("read glb");
    assert!(bytes.len() > 20, "glb too small");
    assert_eq!(&bytes[0..4], &[0x67, 0x6C, 0x54, 0x46], "invalid glb magic");
}
