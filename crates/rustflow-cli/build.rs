use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTFLOW_GIT_REV");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    let revision = env::var("RUSTFLOW_GIT_REV")
        .ok()
        .filter(|value| is_revision(value))
        .or_else(|| {
            let output = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let value = String::from_utf8(output.stdout).ok()?;
            let value = value.trim().to_owned();
            is_revision(&value).then_some(value)
        })
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=RUSTFLOW_GIT_REV={revision}");
}

fn is_revision(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
