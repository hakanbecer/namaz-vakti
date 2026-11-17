fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    // Windows MSVC link arg to embed application manifest
    let manifest_path = std::env::current_dir()
        .map(|d| d.join("app.manifest"))
        .unwrap_or_else(|_| std::path::PathBuf::from("app.manifest"));
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest_path.display());
    println!("cargo:rustc-link-arg=/OPT:REF,ICF");
}
