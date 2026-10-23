//! File name helpers for exports.

/// Lowercase slug: each run of non-alphanumeric characters becomes one `-`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(c.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if out.is_empty() {
        "export".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_cases() {
        assert_eq!(slug("My API"), "my-api");
        assert_eq!(slug("  A/B  "), "a-b");
        assert_eq!(slug("!!!"), "export");
    }
}
