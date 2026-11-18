// Draws the Wolfbeat logo into an .ico and embeds it as the exe's icon (no extra crates).
include!("src/logo.rs");

use std::path::PathBuf;

fn find_rc() -> Option<PathBuf> {
    let base = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&base).ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    versions.sort();
    versions.into_iter().rev().map(|v| v.join("x64").join("rc.exe")).find(|p| p.exists())
}

fn main() {
    println!("cargo:rerun-if-changed=src/logo.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("wolfbeat.ico");
    std::fs::write(&ico, make_ico(&[16, 20, 24, 32, 40, 48, 64, 96, 128, 256])).unwrap();

    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe not found: Wolfbeat.exe will have no embedded icon");
        return;
    };
    let rc_file = out.join("wolfbeat.rc");
    std::fs::write(&rc_file, format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "\\\\"))).unwrap();
    let res = out.join("wolfbeat.res");
    let ok = std::process::Command::new(rc).arg("/nologo").arg("/fo").arg(&res).arg(&rc_file).status().map(|s| s.success()).unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={}", res.display());
    } else {
        println!("cargo:warning=rc.exe failed: Wolfbeat.exe will have no embedded icon");
    }
}
