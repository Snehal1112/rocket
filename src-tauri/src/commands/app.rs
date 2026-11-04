use rocket_shared::error::DomainError;

#[tauri::command]
pub fn get_app_data_dir() -> Result<String, DomainError> {
    let dir = rocket_infra::paths::home_dir()
        .ok_or_else(|| DomainError::Internal("Home directory not found".into()))?
        .join(".rocket-api");
    Ok(dir.to_string_lossy().to_string())
}

/// Set this variable (to any value but "" or "0") to keep WebKitGTK compositing on for Linux.
pub const ENABLE_COMPOSITING_ENV: &str = "ROCKET_ENABLE_COMPOSITING";

/// WebKitGTK turns compositing off when this variable is set to anything but "" or "0".
pub const DISABLE_COMPOSITING_ENV: &str = "WEBKIT_DISABLE_COMPOSITING_MODE";

/// The first WebKitGTK version that paints Rocket correctly with compositing on. With it off,
/// 2.54 leaves parts of the window unpainted, and older versions showed the Scripts editors
/// see-through with it on (checked on 2.44 and 2.54.2, AMD GPU, Wayland).
pub const COMPOSITING_SAFE_FROM: (u32, u32) = (2, 54);

/// True when an environment value means "on", the way WebKitGTK reads its own flags.
fn flag_set(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

/// Whether Rocket should turn WebKitGTK compositing off at startup.
/// `webkit` is the loaded WebKitGTK (major, minor), or `None` off Linux.
/// Only versions older than [`COMPOSITING_SAFE_FROM`] turn it off, unless the opt-in is set.
pub fn should_disable_compositing(
    webkit: Option<(u32, u32)>,
    opt_in: Option<&std::ffi::OsStr>,
) -> bool {
    match webkit {
        Some(version) => version < COMPOSITING_SAFE_FROM && !flag_set(opt_in),
        None => false,
    }
}

/// The WebKitGTK version that is loaded, as (major, minor).
#[cfg(target_os = "linux")]
pub fn webkit_version() -> (u32, u32) {
    // SAFETY: These functions only return constants from the loaded library. They need no
    // GTK setup and take no arguments.
    unsafe {
        (
            webkit2gtk::ffi::webkit_get_major_version(),
            webkit2gtk::ffi::webkit_get_minor_version(),
        )
    }
}

/// Tells the frontend whether blur can paint, so it can keep overlays opaque when it cannot.
/// It reads the flag WebKitGTK reads, so a value set outside Rocket counts too.
#[tauri::command]
pub fn is_compositing_disabled() -> bool {
    cfg!(target_os = "linux") && flag_set(std::env::var_os(DISABLE_COMPOSITING_ENV).as_deref())
}

#[cfg(test)]
mod tests {
    use super::{flag_set, should_disable_compositing};
    use std::ffi::OsStr;

    #[test]
    fn old_webkit_disables_compositing() {
        assert!(should_disable_compositing(Some((2, 44)), None));
        assert!(should_disable_compositing(Some((2, 52)), None));
        assert!(should_disable_compositing(
            Some((2, 52)),
            Some(OsStr::new("0"))
        ));
    }

    #[test]
    fn new_webkit_keeps_compositing_on() {
        assert!(!should_disable_compositing(Some((2, 54)), None));
        assert!(!should_disable_compositing(Some((2, 56)), None));
        assert!(!should_disable_compositing(Some((3, 0)), None));
    }

    #[test]
    fn opt_in_keeps_compositing_on_for_old_webkit() {
        assert!(!should_disable_compositing(
            Some((2, 44)),
            Some(OsStr::new("1"))
        ));
        assert!(!should_disable_compositing(
            Some((2, 44)),
            Some(OsStr::new("yes"))
        ));
    }

    #[test]
    fn other_platforms_never_disable_compositing() {
        assert!(!should_disable_compositing(None, None));
    }

    #[test]
    fn flag_reads_like_webkit() {
        assert!(!flag_set(None));
        assert!(!flag_set(Some(OsStr::new(""))));
        assert!(!flag_set(Some(OsStr::new("0"))));
        assert!(flag_set(Some(OsStr::new("1"))));
    }
}
