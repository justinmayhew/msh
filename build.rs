use std::path::Path;

fn main() {
    // Help Cargo find a Homebrew installed GNU Readline on macOS.
    let path = Path::new("/usr/local/opt/readline/lib");
    if path.exists() {
        println!("cargo:rustc-link-search={}", path.to_str().unwrap());
    }
}
