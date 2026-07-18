/// Current version. See `build.rs` for how this is generated.
pub const VERSION: &str = include_str!(concat!(env!("OUT_DIR"), "/version.txt"));

// Environment variables set by GitHub Actions
pub const GITHUB_REPOSITORY: Option<&str> = option_env!("GITHUB_REPOSITORY");
pub const GITHUB_SERVER_URL: Option<&str> = option_env!("GITHUB_SERVER_URL");
pub const GITHUB_RUN_ID: Option<&str> = option_env!("GITHUB_RUN_ID");
pub const GITHUB_REF_NAME: Option<&str> = option_env!("GITHUB_REF_NAME");
pub const GITHUB_EVENT_NAME: Option<&str> = option_env!("GITHUB_EVENT_NAME");

pub fn branding() -> &'static str {
    if GITHUB_RUN_ID.is_none() {
        return "";
    }
    if GITHUB_REF_NAME == Some(VERSION) {
        return "";
    }
    if GITHUB_REF_NAME == Some("main") {
        "PREVIEW"
    } else {
        "UNOFFICIAL"
    }
}
