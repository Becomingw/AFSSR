fn main() {
    // SQLx tracks existing SQL files, but new migrations also need to invalidate the build.
    println!("cargo:rerun-if-changed=migrations");
}
