//! Põe assets/icon.ico dentro do .exe com o rc.exe do Windows SDK (o mesmo que o Visual Studio usa), sem crate a mais.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let res = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("icon.res");
    let ok = Command::new(rc()).current_dir("assets").args(["/nologo", "/fo"]).arg(&res).arg("icon.rc").status();
    assert!(ok.is_ok_and(|s| s.success()), "o rc.exe não compilou assets/icon.rc");
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

/// O rc.exe x64 da versão mais nova do Windows SDK instalada.
fn rc() -> PathBuf {
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    std::fs::read_dir(kits)
        .expect("Windows SDK não encontrado (vem com o Visual Studio C++)")
        .filter_map(|e| Some(e.ok()?.path().join(r"x64\rc.exe")))
        .filter(|p| p.exists())
        .max()
        .expect("rc.exe do Windows SDK não encontrado")
}
