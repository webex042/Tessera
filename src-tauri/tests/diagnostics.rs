//! The diagnostics bundle carries no content (Stage 9): a store full of a canary token, window
//! titles, app names and people must not leak any of them into the bundle.

use tessara_lib::services::{ModelStatus, Status};
use tessara_lib::views::Counts;
use tsr_core::Sensitive;
use tsr_store::items::{Acquisition, NewItem, NewSource, SourceKind, insert_item, upsert_source};
use tsr_store::migrate::migrate;
use tsr_store::{KEY_LEN, open_encrypted};

#[test]
fn bundle_has_counts_but_no_text_titles_apps_or_people() {
    let d = tempfile::tempdir().unwrap();
    let mut c = open_encrypted(&d.path().join("t.db"), &[4u8; KEY_LEN]).unwrap();
    migrate(&mut c).unwrap();
    let canary = "TSRCANARYDIAG7F3A9";
    for i in 0..5 {
        let s = upsert_source(
            &c,
            &NewSource {
                kind: SourceKind::App,
                app_exe: "SecretBankApp.exe",
                app_name: Some("SecretBankApp"),
                window_title: Some(&format!("{canary} title {i}")),
                url: None,
                file_path: None,
                seen_at: 1,
            },
        )
        .unwrap();
        insert_item(
            &c,
            &NewItem {
                source_id: s,
                started_at: 1 + i,
                ended_at: 1 + i,
                acquisition: Acquisition::Uia,
                text: &Sensitive::new(format!("{canary} body text {i}")),
                simhash: 0,
                lang: None,
                redaction_count: 0,
            },
        )
        .unwrap();
    }
    tsr_store::people::ensure_self(&c, "Zubin Quarterhouse", &[], 1).unwrap();
    let status = Status {
        paused: true,
        capturing: false,
        model: ModelStatus {
            state: "not_set_up".into(),
            backend: None,
        },
        queue: [0, 0, 0],
        counts: Counts {
            open: 0,
            due_today: 0,
            overdue: 0,
            needs_review: 0,
            items_today: 0,
        },
    };
    let v = tessara_lib::diagnostics::bundle(&c, &status, None, None);
    let s = serde_json::to_string(&v).unwrap();
    assert_eq!(v["store"]["items"], 5);
    assert_eq!(v["store"]["people"], 1);
    for leak in [canary, "SecretBankApp", "title", "body text", "Zubin", "Quarterhouse"] {
        assert!(!s.contains(leak), "{leak} leaked into diagnostics: {s}");
    }
}
