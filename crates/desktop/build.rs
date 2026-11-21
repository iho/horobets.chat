fn main() {
    println!("cargo:rerun-if-changed=native/NotificationAgent.swift");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("NotificationAgent");
        let status = std::process::Command::new("swiftc")
            .args(["native/NotificationAgent.swift", "-o"])
            .arg(&output)
            .status()
            .expect("run Swift compiler for notification agent");
        assert!(status.success(), "notification agent compilation failed");
        println!(
            "cargo:rustc-env=MESSENGER_NOTIFICATION_AGENT={}",
            output.display()
        );
    }
}
