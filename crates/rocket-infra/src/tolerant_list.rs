//! A YAML list file that a newer build may have added entries to.
//!
//! An entry this build cannot read is not an error. It is kept as raw YAML and written back
//! unchanged on the next save, so an older build never deletes what a newer build wrote.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_yaml::Value;

/// The entries this build understands, plus the raw entries it does not.
pub(crate) struct TolerantList<T> {
    pub items: Vec<T>,
    kept: Vec<Value>,
}

impl<T> TolerantList<T> {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            kept: Vec::new(),
        }
    }
}

impl<T: DeserializeOwned> TolerantList<T> {
    /// Parses a list file. Only text that is not a YAML list is an error.
    pub fn parse(content: &str, file: &str) -> Result<Self, serde_yaml::Error> {
        let values: Vec<Value> = serde_yaml::from_str(content)?;
        let mut list = Self::empty();
        for value in values {
            match serde_yaml::from_value::<T>(value.clone()) {
                Ok(item) => list.items.push(item),
                Err(e) => {
                    // The error names a field, never a value, so it is safe to log.
                    tracing::warn!(file, error = %e, "keeping an entry this version cannot read");
                    list.kept.push(value);
                }
            }
        }
        Ok(list)
    }
}

impl<T: Serialize> TolerantList<T> {
    /// Renders the known entries followed by the kept raw ones.
    pub fn render(&self) -> Result<String, serde_yaml::Error> {
        let mut values = Vec::with_capacity(self.items.len() + self.kept.len());
        for item in &self.items {
            values.push(serde_yaml::to_value(item)?);
        }
        values.extend(self.kept.iter().cloned());
        serde_yaml::to_string(&values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Row {
        id: String,
        kind: Kind,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Kind {
        A,
        B,
    }

    const FILE: &str =
        "- id: one\n  kind: a\n- id: two\n  kind: z\n  extra: 1\n- id: three\n  kind: b\n";

    #[test]
    fn an_unreadable_entry_is_hidden_from_the_items() {
        let list = TolerantList::<Row>::parse(FILE, "t.yml").expect("parse");
        let ids: Vec<_> = list.items.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["one", "three"]);
    }

    #[test]
    fn an_unreadable_entry_survives_a_render() {
        let mut list = TolerantList::<Row>::parse(FILE, "t.yml").expect("parse");
        list.items.retain(|r| r.id != "one");
        list.items.push(Row {
            id: "four".into(),
            kind: Kind::A,
        });
        let text = list.render().expect("render");
        let again = TolerantList::<Row>::parse(&text, "t.yml").expect("reparse");
        let ids: Vec<_> = again.items.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["three", "four"]);
        assert!(text.contains("id: two") && text.contains("kind: z") && text.contains("extra: 1"));
    }

    #[test]
    fn text_that_is_not_a_list_is_still_an_error() {
        assert!(TolerantList::<Row>::parse("id: one\n", "t.yml").is_err());
    }
}
