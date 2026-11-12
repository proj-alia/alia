fn main() {
    // SQLx embeds migrations in the application and test binaries. Adding a new
    // file must invalidate cached macros, not only editing an existing file.
    println!("cargo:rerun-if-changed=migrations");
}
