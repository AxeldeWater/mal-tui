use std::path::Path;

// Injects the MAL OAuth client id into the binary at compile time so it is not
// hard-coded in the source. Resolution order:
//   1. the `MAL_CLIENT_ID` environment variable (CI release builds set this
//      from a GitHub Actions secret),
//   2. a `MAL_CLIENT_ID=...` line in a local, gitignored `.env` file.
//
// Note: a PKCE/native-app client id is a public identifier, not a true secret
// (it appears in the browser auth URL and can be extracted from any binary).
// This only keeps it out of the committed source, it does not make it private.
fn main() {
    println!("cargo:rerun-if-env-changed=MAL_CLIENT_ID");
    println!("cargo:rerun-if-changed=.env");

    let client_id = std::env::var("MAL_CLIENT_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(read_from_env_file)
        .expect(
            "MAL_CLIENT_ID is not set.\n\
             Set the MAL_CLIENT_ID environment variable (release builds use a \
             GitHub Actions secret), or add a line `MAL_CLIENT_ID=<your id>` to \
             a local .env file (which is gitignored).",
        );

    println!("cargo:rustc-env=MAL_CLIENT_ID={}", client_id.trim());
}

fn read_from_env_file() -> Option<String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let contents = std::fs::read_to_string(Path::new(&manifest_dir).join(".env")).ok()?;
    contents.lines().find_map(|line| {
        let value = line.trim().strip_prefix("MAL_CLIENT_ID=")?;
        let value = value.trim().trim_matches('"').trim_matches('\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}
