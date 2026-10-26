//! A pre-parse that counts what a YAML text would expand to, before any value is built.
//!
//! `serde_yaml` replays an alias target every time the alias is used, so a small text can
//! expand to billions of nodes or gigabytes of strings. The check walks the same events with a
//! visitor that keeps nothing, counts every node and every string byte (replays included) and
//! stops as soon as a limit is passed.

use serde::de::{
    DeserializeSeed, Deserializer, EnumAccess, IgnoredAny, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use std::cell::Cell;

/// Most nodes the reader accepts in one spec.
pub(crate) const READER_MAX_NODES: usize = 1_000_000;
/// Most string bytes (keys and values, replays included) one budget accepts.
pub(crate) const MAX_YAML_BYTES: usize = 25 * 1024 * 1024;

/// Why a text did not pass the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BudgetFail {
    /// A limit was passed.
    Exceeded,
    /// The text is not YAML.
    Invalid,
}

/// Counters that can be shared by several checks, so a group of texts shares one limit.
pub(crate) struct YamlBudget {
    nodes: Cell<usize>,
    bytes: Cell<usize>,
    max_nodes: usize,
    max_bytes: usize,
}

impl YamlBudget {
    pub(crate) fn new(max_nodes: usize, max_bytes: usize) -> Self {
        Self {
            nodes: Cell::new(0),
            bytes: Cell::new(0),
            max_nodes,
            max_bytes,
        }
    }

    fn exceeded(&self) -> bool {
        self.nodes.get() > self.max_nodes || self.bytes.get() > self.max_bytes
    }

    /// Counts what `text` expands to. The counters keep what was added, even on failure.
    pub(crate) fn check(&self, text: &str) -> Result<(), BudgetFail> {
        if self.exceeded() {
            return Err(BudgetFail::Exceeded);
        }
        match Node(self).deserialize(serde_yaml::Deserializer::from_str(text)) {
            Ok(()) => Ok(()),
            Err(_) if self.exceeded() => Err(BudgetFail::Exceeded),
            Err(_) => Err(BudgetFail::Invalid),
        }
    }
}

struct Node<'a>(&'a YamlBudget);

impl Node<'_> {
    fn tick<E: serde::de::Error>(&self, bytes: usize) -> Result<(), E> {
        let budget = self.0;
        budget.nodes.set(budget.nodes.get().saturating_add(1));
        budget.bytes.set(budget.bytes.get().saturating_add(bytes));
        if budget.exceeded() {
            Err(E::custom("too large"))
        } else {
            Ok(())
        }
    }

    fn again(&self) -> Node<'_> {
        Node(self.0)
    }
}

impl<'de> DeserializeSeed<'de> for Node<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Node<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("any YAML value")
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        self.tick(0)
    }

    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        self.tick(0)
    }

    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        self.tick(0)
    }

    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        self.tick(0)
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<(), E> {
        self.tick(value.len())
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        self.tick(0)
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<(), E> {
        self.tick(0)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.tick(0)?;
        deserializer.deserialize_any(self)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.tick(0)?;
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.tick(0)?;
        while seq.next_element_seed(self.again())?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.tick(0)?;
        while map.next_key_seed(self.again())?.is_some() {
            map.next_value_seed(self.again())?;
        }
        Ok(())
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<(), A::Error> {
        self.tick(0)?;
        let (_, variant) = data.variant::<IgnoredAny>()?;
        variant.newtype_variant_seed(self.again())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_nodes_and_bytes_and_stops_early() {
        let budget = YamlBudget::new(100, 1000);
        assert_eq!(budget.check("a: [1, 2, 3]\nb: text\n"), Ok(()));
        assert_eq!(budget.check("a: [1\n"), Err(BudgetFail::Invalid));
        let tight = YamlBudget::new(5, 1000);
        assert_eq!(tight.check("[1, 2, 3, 4, 5, 6, 7]"), Err(BudgetFail::Exceeded));
        let bytes = YamlBudget::new(100, 10);
        assert_eq!(
            bytes.check("a: abcdefghijklmnop\n"),
            Err(BudgetFail::Exceeded)
        );
    }

    #[test]
    fn counters_are_shared_between_checks() {
        let budget = YamlBudget::new(10, 1000);
        assert_eq!(budget.check("[1, 2, 3, 4]"), Ok(()));
        assert_eq!(budget.check("[1, 2, 3, 4]"), Ok(()));
        assert_eq!(budget.check("[1, 2, 3, 4]"), Err(BudgetFail::Exceeded));
        assert_eq!(budget.check("[1]"), Err(BudgetFail::Exceeded));
    }
}
