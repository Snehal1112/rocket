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

/// Most string bytes (keys and values, replays included) one budget accepts.
pub(crate) const MAX_YAML_BYTES: usize = 25 * 1024 * 1024;
/// Nodes allowed per input byte. A text without aliases has fewer nodes than bytes.
const NODES_PER_BYTE: usize = 2;
/// Fewest nodes any budget allows, so small texts are never refused.
const MIN_NODES: usize = 1_000_000;
/// Most nodes any budget allows, however large the input.
const MAX_NODES: usize = 20_000_000;

/// The node limit for inputs of `total_bytes` bytes in all.
///
/// The limit grows with the input, so a large text without aliases passes, while an alias
/// bomb (a small input with a huge expansion) is stopped early.
pub(crate) fn node_limit_for(total_bytes: usize) -> usize {
    total_bytes
        .saturating_mul(NODES_PER_BYTE)
        .clamp(MIN_NODES, MAX_NODES)
}

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
    max_nodes: Cell<usize>,
    max_bytes: usize,
}

impl YamlBudget {
    pub(crate) fn new(max_nodes: usize, max_bytes: usize) -> Self {
        Self {
            nodes: Cell::new(0),
            bytes: Cell::new(0),
            max_nodes: Cell::new(max_nodes),
            max_bytes,
        }
    }

    /// Changes the node limit, for a group of texts whose limit grows as texts are added.
    pub(crate) fn set_max_nodes(&self, max_nodes: usize) {
        self.max_nodes.set(max_nodes);
    }

    fn exceeded(&self) -> bool {
        self.nodes.get() > self.max_nodes.get() || self.bytes.get() > self.max_bytes
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

/// Keeps the large-input tests apart from the tests that time themselves, so the timed ones
/// are not slowed down by the memory the large ones use.
#[cfg(test)]
static HEAVY_TESTS: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Held by a large-input test for its whole run.
#[cfg(test)]
pub(crate) fn heavy_test() -> std::sync::RwLockWriteGuard<'static, ()> {
    HEAVY_TESTS.write().unwrap_or_else(|e| e.into_inner())
}

/// Held by a timed test for its whole run. Timed tests still run together.
#[cfg(test)]
pub(crate) fn timed_test() -> std::sync::RwLockReadGuard<'static, ()> {
    HEAVY_TESTS.read().unwrap_or_else(|e| e.into_inner())
}

/// A valid OpenAPI 3.0 text without aliases, with `operations` operations, short realistic
/// fields and about 2 KB per operation, for tests of large inputs.
#[cfg(test)]
pub(crate) fn large_alias_free_spec(operations: usize) -> String {
    use std::fmt::Write;
    let mut text = String::from("openapi: 3.0.3\ninfo: { title: Big, version: '1' }\npaths:\n");
    for i in 0..operations {
        let _ = write!(
            text,
            "  /items{i}/{{id}}:\n    get:\n      operationId: getItem{i}\n      tags: [items, \
             group{g}]\n      summary: Get item {i}\n      description: Returns item {i} with its \
             fields, links and status. Use the filters to narrow the result.\n      \
             parameters:\n",
            g = i % 50
        );
        for p in 0..8 {
            let _ = writeln!(
                text,
                "        - {{ name: f{p}, in: query, required: false, schema: {{ type: string, \
                 enum: [a, b, c, d, e, f, g, h, i, j] }} }}"
            );
        }
        let _ = write!(
            text,
            "        - {{ name: id, in: path, required: true, schema: {{ type: integer }} }}\n      \
             responses:\n        '200':\n          description: ok\n          content:\n            \
             application/json:\n              schema:\n                type: object\n                \
             properties:\n"
        );
        for f in 0..12 {
            let _ = writeln!(
                text,
                "                  p{f}: {{ type: string, example: v{f}, nullable: true }}"
            );
        }
    }
    text
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
        assert_eq!(
            tight.check("[1, 2, 3, 4, 5, 6, 7]"),
            Err(BudgetFail::Exceeded)
        );
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

    #[test]
    fn the_node_limit_scales_with_the_input() {
        assert_eq!(node_limit_for(0), 1_000_000);
        assert_eq!(node_limit_for(1_000_000), 2_000_000);
        assert_eq!(node_limit_for(25 * 1024 * 1024), 20_000_000);
        assert_eq!(node_limit_for(usize::MAX), 20_000_000);
    }

    #[test]
    fn a_raised_limit_admits_more() {
        let budget = YamlBudget::new(5, 1000);
        assert_eq!(budget.check("[1, 2, 3, 4]"), Ok(()));
        budget.set_max_nodes(10);
        assert_eq!(budget.check("[1, 2, 3, 4]"), Ok(()));
    }
}
