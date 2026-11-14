use crate::context::VariableContext;
use crate::dynamic_vars;
use std::collections::{HashMap, HashSet};

/// How many levels of nested names are filled below the top value.
pub const MAX_NESTED_DEPTH: usize = 3;

/// The most bytes one expanded value may reach through nested fills.
pub const MAX_NESTED_BYTES: usize = 64 * 1024;

/// The most bytes scanned and filled, over all levels, for one top value.
pub const MAX_NESTED_WORK: usize = 1024 * 1024;

/// The work charged for every fill attempt, on top of the bytes scanned and filled.
const NESTED_FILL_COST: usize = 64;

/// Returns `ctx.flatten()` with `extra` (Flow variables) laid on top, where the stored values
/// have their `{{placeholders}}` filled.
///
/// A name is filled only when its winning layer is request, folder, env, collection or global.
/// A name won by runtime, external secrets or `extra` keeps its value.
/// Lookups use the same merged map, except vault names and `process.env.` names, which stay as
/// written. A nested name is expanded too, at most `MAX_NESTED_DEPTH` levels below the top value.
/// Past that, or on a name already on the current path (a cycle), the placeholder stays.
/// A `$name` is filled from `dynamic`, which is generated and stored on first use.
/// A name that is a RocketVault key is never filled inside a value, even when a stored scope
/// defines the same name.
/// While one value is expanded, once a fill would make its output longer than
/// `MAX_NESTED_BYTES`, that placeholder and every later one in the value stay as written.
/// Each top value also has a work budget of `MAX_NESTED_WORK` bytes (scanned plus filled, over all
/// levels). Once it is spent, every remaining placeholder in that value stays as written.
/// Every fill attempt also costs a fixed 64 bytes of that budget, so many tiny fills stop too.
/// A stored value longer than the space left is refused before it is copied.
/// A stored top value longer than `MAX_NESTED_WORK` is not expanded at all, and a placeholder
/// after `MAX_NESTED_BYTES` of literal text stays as written.
/// An unknown name or `$name` stays. Syntax and trimming follow `resolver::resolve`.
pub fn expand_nested(
    ctx: &VariableContext,
    extra: &HashMap<String, String>,
    dynamic: &mut HashMap<String, String>,
) -> HashMap<String, String> {
    expand_nested_recording(ctx, extra, dynamic, &mut HashSet::new())
}

/// `expand_nested`, and it also records in `secret_fills` the text put in for a placeholder that
/// names a key of `ctx.secret_keys` inside another name's value.
///
/// A text is recorded only when it differs from that secret's raw value and from its own fully
/// expanded value, which the caller masks already. That is the case when the fill was cut (depth
/// or budget) and part of the secret stays as written.
pub fn expand_nested_recording(
    ctx: &VariableContext,
    extra: &HashMap<String, String>,
    dynamic: &mut HashMap<String, String>,
    secret_fills: &mut HashSet<String>,
) -> HashMap<String, String> {
    let mut merged = ctx.flatten();
    merged.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
    let mut out = merged.clone();
    let mut expander = Expander {
        ctx,
        extra,
        merged: &merged,
        dynamic,
        work: 0,
        secret_candidates: Vec::new(),
        #[cfg(test)]
        fills: 0,
        #[cfg(test)]
        clones: 0,
    };
    for (name, value) in &merged {
        if expander.is_expandable(name) {
            let mut path = vec![name.as_str()];
            let (filled, _) = expander.expand_top(value, &mut path);
            out.insert(name.clone(), filled);
        }
    }
    for (name, text) in std::mem::take(&mut expander.secret_candidates) {
        if merged.get(&name) != Some(&text) && out.get(&name) != Some(&text) {
            secret_fills.insert(text);
        }
    }
    out
}

/// Counters of one top-value expansion, for tests.
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Stats {
    pub work: usize,
    pub fills: usize,
    pub clones: usize,
}

#[cfg(test)]
pub(crate) fn expand_one(
    ctx: &VariableContext,
    extra: &HashMap<String, String>,
    dynamic: &mut HashMap<String, String>,
    name: &str,
) -> (String, Stats) {
    let mut merged = ctx.flatten();
    merged.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
    let value = merged.get(name).cloned().unwrap_or_default();
    let mut expander = Expander {
        ctx,
        extra,
        merged: &merged,
        dynamic,
        work: 0,
        secret_candidates: Vec::new(),
        fills: 0,
        clones: 0,
    };
    let mut path = vec![name];
    let (out, work) = expander.expand_top(&value, &mut path);
    let stats = Stats {
        work,
        fills: expander.fills,
        clones: expander.clones,
    };
    (out, stats)
}

struct Expander<'a> {
    ctx: &'a VariableContext,
    extra: &'a HashMap<String, String>,
    merged: &'a HashMap<String, String>,
    dynamic: &'a mut HashMap<String, String>,
    /// Bytes scanned and filled for the current top value.
    work: usize,
    /// Secret names filled inside another value, with the text put in.
    secret_candidates: Vec<(String, String)>,
    /// Fill attempts for the current top value.
    #[cfg(test)]
    fills: usize,
    /// Copies of a non-expandable stored value for the current top value.
    #[cfg(test)]
    clones: usize,
}

impl<'a> Expander<'a> {
    /// True when the winning layer of `name` holds a stored value that may be expanded.
    fn is_expandable(&self, name: &str) -> bool {
        let ctx = self.ctx;
        if self.extra.contains_key(name) || ctx.runtime.contains_key(name) {
            return false;
        }
        if ctx.request.contains_key(name) {
            return true;
        }
        // A row column is used as written, even when it shadows a folder or env value.
        if ctx.data.contains_key(name) {
            return false;
        }
        if [&ctx.folder, &ctx.env]
            .iter()
            .any(|layer| layer.contains_key(name))
        {
            return true;
        }
        if ctx.external_secrets.contains_key(name) {
            return false;
        }
        ctx.collection.contains_key(name) || ctx.global_env.contains_key(name)
    }

    /// Expands one top value with a fresh work budget. Returns the text and the work used.
    fn expand_top<'n>(&mut self, value: &str, path: &mut Vec<&'n str>) -> (String, usize)
    where
        'a: 'n,
    {
        self.work = 0;
        #[cfg(test)]
        {
            self.fills = 0;
            self.clones = 0;
        }
        let out = self.expand_value(value, path, 0, MAX_NESTED_BYTES);
        (out, self.work)
    }

    /// Fills the placeholders of `value`. `depth` is the level of `value` below the top value.
    /// A fill is accepted only while the output stays within `limit` bytes.
    fn expand_value<'n>(
        &mut self,
        value: &str,
        path: &mut Vec<&'n str>,
        depth: usize,
        limit: usize,
    ) -> String
    where
        'a: 'n,
    {
        self.work = self.work.saturating_add(value.len());
        let mut output = String::with_capacity(value.len());
        let mut capped = false;
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '{' || chars.peek() != Some(&'{') {
                output.push(ch);
                continue;
            }
            chars.next();
            let mut raw = String::new();
            let mut closed = false;
            while let Some(inner) = chars.next() {
                if inner == '}' && chars.peek() == Some(&'}') {
                    chars.next();
                    closed = true;
                    break;
                }
                raw.push(inner);
            }
            if !closed {
                output.push_str("{{");
                output.push_str(&raw);
                continue;
            }
            let filled = if capped || self.work.saturating_add(NESTED_FILL_COST) > MAX_NESTED_WORK {
                None
            } else {
                self.work += NESTED_FILL_COST;
                #[cfg(test)]
                {
                    self.fills += 1;
                }
                let space = limit.saturating_sub(output.len());
                self.fill(raw.trim(), path, depth, space)
            };
            match filled {
                Some(filled)
                    if output.len() + filled.len() <= limit
                        && self.work.saturating_add(filled.len()) <= MAX_NESTED_WORK =>
                {
                    self.work = self.work.saturating_add(filled.len());
                    output.push_str(&filled)
                }
                other => {
                    capped = capped || other.is_some();
                    output.push_str("{{");
                    output.push_str(&raw);
                    output.push_str("}}");
                }
            }
        }
        output
    }

    /// The text for one placeholder name, or `None` when it must stay as written.
    /// A value longer than `space` is refused before it is copied.
    fn fill<'n>(
        &mut self,
        name: &str,
        path: &mut Vec<&'n str>,
        depth: usize,
        space: usize,
    ) -> Option<String>
    where
        'a: 'n,
    {
        if let Some(dyn_name) = name.strip_prefix('$') {
            if let Some(cached) = self.dynamic.get(name) {
                return Some(cached.clone());
            }
            let generated = dynamic_vars::generate(dyn_name)?;
            self.dynamic.insert(name.to_string(), generated.clone());
            return Some(generated);
        }
        if name.starts_with("process.env.") || self.ctx.external_secrets.contains_key(name) {
            return None;
        }
        if depth >= MAX_NESTED_DEPTH || path.contains(&name) {
            return None;
        }
        let merged: &'a HashMap<String, String> = self.merged;
        let (key, value) = merged.get_key_value(name)?;
        if self.work.saturating_add(value.len()) > MAX_NESTED_WORK {
            return None;
        }
        if !self.is_expandable(name) {
            if value.len() > space {
                return None;
            }
            #[cfg(test)]
            {
                self.clones += 1;
            }
            return Some(value.clone());
        }
        path.push(key.as_str());
        let filled = self.expand_value(value, path, depth + 1, space);
        path.pop();
        if self.ctx.secret_keys.contains(name) {
            self.secret_candidates
                .push((name.to_string(), filled.clone()));
        }
        Some(filled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn expand(ctx: &VariableContext) -> HashMap<String, String> {
        expand_nested(ctx, &HashMap::new(), &mut HashMap::new())
    }

    #[test]
    fn an_env_value_fills_a_nested_name() {
        let ctx = VariableContext {
            env: m(&[("baseUrl", "https://{{host}}"), ("host", "api.test")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["baseUrl"], "https://api.test");
    }

    #[test]
    fn nested_names_follow_scope_order() {
        // host is in collection and env; env wins inside the expanded value too.
        let ctx = VariableContext {
            collection: m(&[("baseUrl", "https://{{host}}"), ("host", "col.test")]),
            env: m(&[("host", "env.test")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["baseUrl"], "https://env.test");
    }

    #[test]
    fn three_levels_fill_and_the_fourth_stays() {
        let ctx = VariableContext {
            env: m(&[
                ("a", "{{b}}"),
                ("b", "{{c}}"),
                ("c", "{{d}}"),
                ("d", "{{e}}"),
                ("e", "end"),
            ]),
            ..Default::default()
        };
        let out = expand(&ctx);
        assert_eq!(out["b"], "end"); // b -> c -> d -> e: 3 levels below b.
        assert_eq!(out["a"], "{{e}}"); // a -> b -> c -> d: the 4th level stays.
    }

    #[test]
    fn a_cycle_leaves_the_inner_placeholder() {
        let ctx = VariableContext {
            env: m(&[("a", "x{{b}}"), ("b", "y{{a}}")]),
            ..Default::default()
        };
        let out = expand(&ctx);
        assert_eq!(out["a"], "xy{{a}}");
        assert_eq!(out["b"], "yx{{b}}");
    }

    #[test]
    fn a_self_reference_stays() {
        let ctx = VariableContext {
            env: m(&[("a", "{{a}}!")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["a"], "{{a}}!");
    }

    #[test]
    fn a_vault_name_inside_a_value_stays() {
        let ctx = VariableContext {
            env: m(&[("x", "Bearer {{rv.secret}}")]),
            external_secrets: m(&[("rv.secret", "vault-value-123")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["x"], "Bearer {{rv.secret}}");
    }

    #[test]
    fn a_process_env_name_inside_a_value_stays() {
        let ctx = VariableContext {
            env: m(&[("x", "{{process.env.HOME}}")]),
            process_env: m(&[("HOME", "/h")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["x"], "{{process.env.HOME}}");
    }

    #[test]
    fn runtime_and_extra_values_are_not_expanded() {
        let ctx = VariableContext {
            env: m(&[("host", "api.test")]),
            runtime: m(&[("r", "https://{{host}}")]),
            ..Default::default()
        };
        let out = expand_nested(&ctx, &m(&[("f", "{{host}}")]), &mut HashMap::new());
        assert_eq!(out["r"], "https://{{host}}");
        assert_eq!(out["f"], "{{host}}");
    }

    #[test]
    fn a_stored_value_can_use_a_runtime_value() {
        let ctx = VariableContext {
            env: m(&[("auth", "Bearer {{token}}")]),
            runtime: m(&[("token", "t-1")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["auth"], "Bearer t-1");
    }

    #[test]
    fn a_dynamic_name_is_made_once_per_cache() {
        let ctx = VariableContext {
            env: m(&[("a", "{{$guid}}"), ("b", "{{$guid}}")]),
            ..Default::default()
        };
        let mut dynamic = HashMap::new();
        let out = expand_nested(&ctx, &HashMap::new(), &mut dynamic);
        assert_eq!(out["a"], out["b"]);
        assert_eq!(dynamic["$guid"], out["a"]);
        let again = expand_nested(&ctx, &HashMap::new(), &mut dynamic);
        assert_eq!(again["a"], out["a"]);
    }

    #[test]
    fn unknown_names_stay() {
        let ctx = VariableContext {
            env: m(&[("a", "{{nope}} {{$nope}}")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["a"], "{{nope}} {{$nope}}");
    }

    #[test]
    fn a_huge_fan_out_is_capped() {
        let eight = |n: &str| "{{".to_string() + n + "}}";
        let ctx = VariableContext {
            env: m(&[
                ("a", &eight("b").repeat(8)),
                ("b", &eight("c").repeat(8)),
                ("c", &eight("d").repeat(8)),
                ("d", &eight("e").repeat(8)),
                ("e", &"x".repeat(200)),
            ]),
            ..Default::default()
        };
        // Only b reaches the cap: uncapped it is 102,400 bytes with no placeholder left.
        let out = expand(&ctx);
        assert!(out["b"].len() <= MAX_NESTED_BYTES);
        assert!(out["b"].contains("{{"));
    }

    #[test]
    fn a_vault_key_shadowed_by_env_stays() {
        let ctx = VariableContext {
            env: m(&[("k", "{{rv.secret}}"), ("rv.secret", "env-val")]),
            external_secrets: m(&[("rv.secret", "vault-val")]),
            ..Default::default()
        };
        assert_eq!(expand(&ctx)["k"], "{{rv.secret}}");
    }

    fn rep(name: &str, k: usize) -> String {
        format!("{{{{{name}}}}}").repeat(k)
    }

    #[test]
    fn a_shrinking_fan_out_stays_within_the_work_budget() {
        let ctx = VariableContext {
            env: m(&[
                ("a", &rep("b", 300)),
                ("b", &rep("c", 300)),
                ("c", &rep("d", 300)),
                ("d", ""),
            ]),
            ..Default::default()
        };
        let (out, stats) = expand_one(&ctx, &HashMap::new(), &mut HashMap::new(), "a");
        assert!(stats.work <= MAX_NESTED_WORK, "work was {}", stats.work);
        // Without the budget every fill is empty and nothing is left.
        assert!(out.contains("{{"));
    }

    #[test]
    fn a_huge_runtime_value_is_not_cloned_past_the_cap() {
        let ctx = VariableContext {
            env: m(&[
                ("a", &rep("b", 300)),
                ("b", &rep("c", 300)),
                ("c", "{{big}}"),
            ]),
            runtime: m(&[("big", &"z".repeat(10 * 1024 * 1024))]),
            ..Default::default()
        };
        let (out, stats) = expand_one(&ctx, &HashMap::new(), &mut HashMap::new(), "a");
        assert!(out.len() <= MAX_NESTED_BYTES);
        assert_eq!(stats.clones, 0);
        assert!(stats.fills <= MAX_NESTED_WORK / NESTED_FILL_COST);
    }

    #[test]
    fn many_names_with_empty_leaves_stay_fast() {
        let mut env = vec![
            ("b".to_string(), rep("c", 60)),
            ("c".to_string(), rep("d", 60)),
            ("d".to_string(), String::new()),
        ];
        for i in 0..3 {
            env.push((format!("t{i}"), rep("b", 60)));
        }
        let ctx = VariableContext {
            env: env.into_iter().collect(),
            ..Default::default()
        };
        for i in 0..3 {
            let (_, stats) =
                expand_one(&ctx, &HashMap::new(), &mut HashMap::new(), &format!("t{i}"));
            assert!(
                stats.fills <= MAX_NESTED_WORK / NESTED_FILL_COST,
                "fills were {}",
                stats.fills
            );
        }
    }

    #[test]
    fn a_value_bigger_than_the_space_left_is_not_cloned() {
        let ctx = VariableContext {
            env: m(&[("a", &rep("b", 4)), ("b", "{{big}}")]),
            runtime: m(&[("big", &"z".repeat(900 * 1024))]),
            ..Default::default()
        };
        let (out, stats) = expand_one(&ctx, &HashMap::new(), &mut HashMap::new(), "a");
        assert_eq!(stats.clones, 0);
        assert!(out.len() <= MAX_NESTED_BYTES);
    }

    fn recorded(ctx: &VariableContext) -> HashSet<String> {
        let mut fills = HashSet::new();
        expand_nested_recording(ctx, &HashMap::new(), &mut HashMap::new(), &mut fills);
        fills
    }

    fn secret_ctx(env: HashMap<String, String>, secrets: &[&str]) -> VariableContext {
        VariableContext {
            env,
            secret_keys: secrets.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_secret_cut_by_depth_inside_another_value_is_recorded() {
        let ctx = secret_ctx(
            m(&[
                ("s", "lit-part-123{{a}}"),
                ("a", "{{b}}"),
                ("b", "{{c}}"),
                ("c", "x"),
                ("next", "{{s}}"),
            ]),
            &["s"],
        );
        let fills = recorded(&ctx);
        assert_eq!(fills, HashSet::from(["lit-part-123{{c}}".to_string()]));
    }

    #[test]
    fn a_fully_filled_secret_inside_another_value_is_not_recorded() {
        let ctx = secret_ctx(
            m(&[("s", "lit-part-123{{a}}"), ("a", "x"), ("next", "{{s}}")]),
            &["s"],
        );
        assert!(recorded(&ctx).is_empty());
    }

    #[test]
    fn a_cut_non_secret_inside_another_value_is_not_recorded() {
        let ctx = secret_ctx(
            m(&[
                ("s", "lit-part-123{{a}}"),
                ("a", "{{b}}"),
                ("b", "{{c}}"),
                ("c", "x"),
                ("next", "{{s}}"),
            ]),
            &[],
        );
        assert!(recorded(&ctx).is_empty());
    }
}
