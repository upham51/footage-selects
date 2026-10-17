// The product name lives in one place: `productName` in src-tauri/tauri.conf.json.
// This makes it available to the engine as the PRODUCT_NAME constant.
fn main() {
    let conf = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/tauri.conf.json");
    println!("cargo:rerun-if-changed={}", conf.display());
    let text = std::fs::read_to_string(&conf).expect("src-tauri/tauri.conf.json must exist");
    let json: serde_json::Value = serde_json::from_str(&text).expect("tauri.conf.json must be valid JSON");
    let name = json["productName"].as_str().expect("tauri.conf.json needs a productName");
    let version = json["version"].as_str().expect("tauri.conf.json needs a version");
    println!("cargo:rustc-env=PRODUCT_NAME={name}");
    println!("cargo:rustc-env=PRODUCT_VERSION={version}");
}
