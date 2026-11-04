/// Increments the patch segment of a semver-like version string.
/// E.g. `"1.2.3"` becomes `"1.2.4"`. Other shapes get a `-copy` suffix.
pub fn bump_patch_version(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if let [major, minor, patch] = parts.as_slice() {
        // A patch that cannot be parsed or incremented takes the fallback.
        if let Some(next) = patch.parse::<u32>().ok().and_then(|p| p.checked_add(1)) {
            return format!("{major}.{minor}.{next}");
        }
    }
    format!("{version}-copy")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_patch_version_increments_the_patch() {
        assert_eq!(bump_patch_version("1.2.3"), "1.2.4");
        assert_eq!(bump_patch_version("0.0.0"), "0.0.1");
    }

    #[test]
    fn bump_patch_version_falls_back_for_other_shapes() {
        assert_eq!(bump_patch_version("v2"), "v2-copy");
        assert_eq!(bump_patch_version("1.0"), "1.0-copy");
        assert_eq!(bump_patch_version(""), "-copy");
        assert_eq!(bump_patch_version("1.2.3-beta"), "1.2.3-beta-copy");
    }

    #[test]
    fn bump_patch_version_does_not_overflow() {
        assert_eq!(bump_patch_version("1.2.4294967295"), "1.2.4294967295-copy");
    }
}
