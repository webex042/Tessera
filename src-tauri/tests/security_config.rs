//! Tauri security configuration (Stage 7 task 7): fails if a change loosens it.

use std::path::Path;

fn read(p: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(p)).unwrap()
}

#[test]
fn csp_allows_only_the_app_itself_and_ipc() {
    let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    let sec = &conf["app"]["security"];
    let csp = sec["csp"].as_object().expect("CSP is set");
    for (k, v) in csp {
        let v = v.as_str().unwrap();
        assert!(!v.contains("unsafe-inline") && !v.contains("unsafe-eval"), "{k}: {v}");
        assert!(
            !v.contains("http://*") && !v.contains("https:") && !v.contains('*'),
            "{k}: {v}"
        );
    }
    assert_eq!(csp["default-src"], "'self'");
    assert_eq!(csp["connect-src"], "ipc: http://ipc.localhost");
    assert_eq!(csp["object-src"], "'none'");
    assert_eq!(sec["freezePrototype"], true);
    assert_eq!(conf["app"]["withGlobalTauri"], false);
}

#[test]
fn the_notch_may_only_listen_to_events() {
    let cap: serde_json::Value = serde_json::from_str(&read("capabilities/notch.json")).unwrap();
    let perms: Vec<&str> = cap["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(perms, ["core:event:allow-listen", "core:event:allow-unlisten"]);
    let dir = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities")).unwrap();
    assert_eq!(dir.count(), 1, "one capability file");
}

#[test]
fn devtools_are_not_compiled_into_release_builds() {
    let manifest = read("Cargo.toml");
    let workspace = read("../Cargo.toml");
    for m in [&manifest, &workspace] {
        for line in m.lines().filter(|l| l.trim_start().starts_with("tauri ")) {
            assert!(!line.contains("devtools"), "tauri devtools feature enabled: {line}");
        }
    }
}
