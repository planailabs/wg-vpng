use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Cache-busting query param for the tailwind stylesheet.
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={ts}");
}
