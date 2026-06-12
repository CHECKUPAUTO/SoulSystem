fn main() {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();
    if let Ok(out) = output {
        let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !hash.is_empty() {
            println!("cargo:rustc-env=GIT_HASH={}", hash);
        }
    }
}
