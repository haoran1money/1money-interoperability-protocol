use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let code_dir = manifest_dir
        .parent()
        .expect("crates directory")
        .parent()
        .expect("code directory")
        .to_path_buf();
    let solidity_root = code_dir.join("solidity");
    let foundry_manifest = code_dir.join("foundry.toml");
    let foundry_lock = solidity_root.join("foundry.lock");

    println!("cargo:rerun-if-changed={}", foundry_manifest.display());
    if foundry_lock.exists() {
        println!("cargo:rerun-if-changed={}", foundry_lock.display());
    }
    emit_solidity_rerun(&solidity_root.join("src"));

    let status = Command::new("forge")
        .arg("build")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(&code_dir)
        .status()
        .expect("failed to spawn `forge` process");

    if !status.success() {
        panic!("`forge build` failed with status {status}");
    }

    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let stamp_path = PathBuf::from(out_dir).join("forge-build.stamp");
        if let Some(parent) = stamp_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        fs::write(&stamp_path, nanos.to_string()).expect("write forge build stamp");
        println!("cargo:rerun-if-changed={}", stamp_path.display());
    }
}

fn emit_solidity_rerun(path: &Path) {
    if !path.exists() {
        return;
    }

    let mut queue = VecDeque::from([path.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => queue.push_back(entry_path),
                Ok(ft)
                    if ft.is_file() && entry_path.extension().is_some_and(|ext| ext == "sol") =>
                {
                    println!("cargo:rerun-if-changed={}", entry_path.display());
                }
                _ => {}
            }
        }
    }
}
