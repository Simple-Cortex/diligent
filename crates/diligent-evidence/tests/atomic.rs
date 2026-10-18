//! `atomic_write` and `append_line`.

use diligent_evidence::{append_line, atomic_write, atomic_write_new, atomic_write_with};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!(
            "diligent-evidence-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn a_write_creates_the_file_and_leaves_no_temp() {
    let dir = TempDir::new("write");
    let target = dir.path().join("a.json");
    atomic_write(&target, b"hello").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"hello");
    assert_eq!(names(dir.path()), vec!["a.json"]);
}

#[test]
fn a_failed_fill_leaves_no_final_file_and_no_temp() {
    let dir = TempDir::new("fail-new");
    let target = dir.path().join("a.json");
    let mut temp_seen = Vec::new();
    let result = atomic_write_with(&target, |file| {
        file.write_all(b"half of the con")?;
        temp_seen = names(dir.path());
        Err(io::Error::other("disk full"))
    });
    assert!(result.is_err());
    assert!(!target.exists(), "no partial file under the final name");
    assert!(names(dir.path()).is_empty(), "the temp file is removed");
    // While the write was in flight the data lived in a dot-temp file.
    assert_eq!(temp_seen.len(), 1);
    assert!(
        temp_seen[0].starts_with(".a.json.tmp-"),
        "temp name was {}",
        temp_seen[0]
    );
    assert!(
        temp_seen[0].contains(&format!("-{}-", std::process::id())),
        "temp name carries the pid"
    );
}

#[test]
fn a_failed_overwrite_keeps_the_old_content() {
    let dir = TempDir::new("fail-old");
    let target = dir.path().join("a.json");
    atomic_write(&target, b"old").unwrap();
    let result = atomic_write_with(&target, |file| {
        file.write_all(b"new but incomp")?;
        Err(io::Error::other("killed"))
    });
    assert!(result.is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(names(dir.path()), vec!["a.json"]);
}

#[test]
fn a_directory_in_the_way_is_an_error_and_cleans_up() {
    let dir = TempDir::new("dir-in-way");
    let target = dir.path().join("taken");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("inside"), b"x").unwrap();
    assert!(atomic_write(&target, b"data").is_err());
    assert!(target.is_dir());
    assert_eq!(names(dir.path()), vec!["taken"], "no temp file left behind");
}

#[test]
fn a_missing_parent_directory_is_an_error() {
    let dir = TempDir::new("no-parent");
    let target = dir.path().join("no/such/dir/a.json");
    assert!(atomic_write(&target, b"data").is_err());
    assert!(!dir.path().join("no").exists());
}

#[test]
fn an_empty_write_is_a_real_zero_byte_file() {
    let dir = TempDir::new("empty");
    let target = dir.path().join("e.bin");
    atomic_write(&target, b"").unwrap();
    assert_eq!(fs::metadata(&target).unwrap().len(), 0);
}

#[test]
fn overwriting_replaces_the_whole_file() {
    let dir = TempDir::new("replace");
    let target = dir.path().join("a");
    atomic_write(&target, b"a much longer first version").unwrap();
    atomic_write(&target, b"short").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"short");
}

#[test]
fn write_new_never_overwrites() {
    let dir = TempDir::new("new");
    let target = dir.path().join("STORE.json");
    assert!(atomic_write_new(&target, b"first").unwrap());
    assert!(!atomic_write_new(&target, b"second").unwrap());
    assert_eq!(fs::read(&target).unwrap(), b"first");
    assert_eq!(
        names(dir.path()),
        vec!["STORE.json"],
        "the loser's temp file is removed"
    );
}

#[test]
fn append_adds_one_newline_terminated_line_each_time() {
    let dir = TempDir::new("append");
    let target = dir.path().join("log.jsonl");
    append_line(&target, r#"{"a":1}"#).unwrap();
    append_line(&target, r#"{"a":2}"#).unwrap();
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "{\"a\":1}\n{\"a\":2}\n"
    );
}

#[test]
fn append_refuses_a_line_with_a_newline() {
    let dir = TempDir::new("append-nl");
    let target = dir.path().join("log.jsonl");
    assert!(append_line(&target, "a\nb").is_err());
    assert!(append_line(&target, "a\rb").is_err());
    assert!(!target.exists() || fs::read(&target).unwrap().is_empty());
}

#[test]
fn threads_appending_to_one_file_never_interleave() {
    let dir = TempDir::new("threads");
    let target = dir.path().join("log.jsonl");
    let pad = "x".repeat(6000);
    std::thread::scope(|scope| {
        for t in 0..4 {
            let target = &target;
            let pad = &pad;
            scope.spawn(move || {
                for i in 0..100 {
                    append_line(target, &format!(r#"{{"t":{t},"i":{i},"pad":"{pad}"}}"#)).unwrap();
                }
            });
        }
    });
    let text = fs::read_to_string(&target).unwrap();
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let value: serde_json::Value =
            serde_json::from_str(line).expect("every line is whole JSON");
        assert!(seen.insert((value["t"].as_i64().unwrap(), value["i"].as_i64().unwrap())));
    }
    assert_eq!(seen.len(), 400);
}

/// Two real processes appending to one file. The parent re-runs this test
/// binary twice with `DILIGENT_EVIDENCE_APPEND_TO` set; each child appends.
#[test]
fn two_processes_appending_to_one_file_never_interleave() {
    if let Ok(target) = std::env::var("DILIGENT_EVIDENCE_APPEND_TO") {
        let who = std::env::var("DILIGENT_EVIDENCE_APPEND_AS").unwrap_or_default();
        let pad = "y".repeat(9000);
        for i in 0..150 {
            append_line(
                Path::new(&target),
                &format!(r#"{{"who":"{who}","i":{i},"pad":"{pad}"}}"#),
            )
            .unwrap();
        }
        return;
    }
    let dir = TempDir::new("processes");
    let target = dir.path().join("log.jsonl");
    let exe = std::env::current_exe().unwrap();
    let children: Vec<_> = ["one", "two"]
        .iter()
        .map(|who| {
            std::process::Command::new(&exe)
                .args([
                    "--exact",
                    "two_processes_appending_to_one_file_never_interleave",
                    "--test-threads=1",
                ])
                .env("DILIGENT_EVIDENCE_APPEND_TO", &target)
                .env("DILIGENT_EVIDENCE_APPEND_AS", who)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let text = fs::read_to_string(&target).unwrap();
    let mut count = 0;
    for line in text.lines() {
        let value: serde_json::Value =
            serde_json::from_str(line).expect("every line is whole JSON");
        assert!(value["who"] == "one" || value["who"] == "two");
        count += 1;
    }
    assert_eq!(count, 300);
}
