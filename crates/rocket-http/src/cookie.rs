use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<String>,
}

/// CookieJar aggregate — cookies grouped by domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieJar {
    pub domain: String,
    pub cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            cookies: Vec::new(),
        }
    }

    pub fn add(&mut self, cookie: Cookie) {
        // Replace existing cookie with same name.
        if let Some(existing) = self.cookies.iter_mut().find(|c| c.name == cookie.name) {
            *existing = cookie;
        } else {
            self.cookies.push(cookie);
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.cookies.retain(|c| c.name != name);
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.iter().find(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cookie(name: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "val".into(),
            domain: "example.com".into(),
            path: "/".into(),
            secure: false,
            http_only: false,
            expires: None,
        }
    }

    #[test]
    fn add_and_get_cookie() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        assert_eq!(jar.get("session").unwrap().value, "val");
    }

    #[test]
    fn add_replaces_existing() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        let mut updated = sample_cookie("session");
        updated.value = "new_val".into();
        jar.add(updated);
        assert_eq!(jar.cookies.len(), 1);
        assert_eq!(jar.get("session").unwrap().value, "new_val");
    }

    #[test]
    fn remove_cookie() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        jar.remove("session");
        assert!(jar.get("session").is_none());
    }

    #[test]
    fn clear_all() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("a"));
        jar.add(sample_cookie("b"));
        jar.clear();
        assert!(jar.cookies.is_empty());
    }
}
