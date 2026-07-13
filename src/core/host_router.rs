use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use tracing::warn;

thread_local! {
    static MATCHES_BUF: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
}

pub struct HostRouter {
    exact_matches: HashMap<String, usize>,
    leading_wildcards: HashMap<String, usize>,
    trailing_wildcards: HashMap<String, usize>,
    double_star_leading: BTreeMap<usize, HashMap<String, usize>>,
    double_star_trailing: BTreeMap<usize, HashMap<String, usize>>,
    simple_internal_wildcards: Vec<(Vec<String>, Vec<String>, usize)>,
    internal_double_star_wildcards: Vec<(Vec<String>, Vec<String>, usize)>,
    multi_star_trie: LabelTrie,
    complex_patterns: Vec<(Vec<String>, usize)>,
    catch_all: Option<usize>,
}

impl HostRouter {
    pub fn new() -> Self {
        Self {
            exact_matches: HashMap::new(),
            leading_wildcards: HashMap::new(),
            trailing_wildcards: HashMap::new(),
            double_star_leading: BTreeMap::new(),
            double_star_trailing: BTreeMap::new(),
            simple_internal_wildcards: Vec::new(),
            internal_double_star_wildcards: Vec::new(),
            multi_star_trie: LabelTrie::new(),
            complex_patterns: Vec::new(),
            catch_all: None,
        }
    }

    pub fn add_pattern(&mut self, pattern: &str, index: usize) {
        if pattern == "*" || pattern == "**" {
            // The simple symbols "*" and "**" mean a match for all domains.
            if self.catch_all.is_some() {
                warn!(index=%index, "There are multiple 'catch_all' wildcards configured on one of the ports.");
                return;
            }
            self.catch_all = Some(index);
            return;
        }

        let star_count = pattern.matches('*').count();

        if star_count == 0 {
            self.exact_matches.insert(pattern.to_string(), index);
            return;
        } else if star_count == 1 && pattern.starts_with("*.") {
            let domain_part = &pattern[2..];
            self.leading_wildcards
                .insert(domain_part.to_string(), index);
            return;
        } else if star_count == 1 && pattern.ends_with(".*") {
            let domain_part = &pattern[..pattern.len() - 2];
            self.trailing_wildcards
                .insert(domain_part.to_string(), index);
            return;
        }

        let labels: Vec<&str> = pattern.split('.').collect();
        let has_double_star = labels.contains(&"**");
        let has_single_star = labels.contains(&"*");

        if labels.first() == Some(&"**") && labels[1..].iter().all(|l| !l.contains('*')) {
            let known_parts = labels[1..].join(".");
            self.double_star_leading
                .entry(labels.len() - 1)
                .or_default()
                .insert(known_parts, index);
            return;
        }

        if labels.last() == Some(&"**")
            && labels[..labels.len() - 1].iter().all(|l| !l.contains('*'))
        {
            let known_parts = labels[..labels.len() - 1].join(".");
            self.double_star_trailing
                .entry(labels.len() - 1)
                .or_default()
                .insert(known_parts, index);
            return;
        }

        if has_double_star && has_single_star {
            let owned_labels: Vec<String> = pattern.split('.').map(String::from).collect();
            self.complex_patterns.push((owned_labels, index));
            return;
        }

        let single_wildcard_count = labels.iter().filter(|label| **label == "*").count();
        if single_wildcard_count == 1 && !has_double_star {
            let wildcard_pos = labels.iter().position(|label| *label == "*");
            if let Some(pos) = wildcard_pos
                && pos > 0
                && pos < labels.len() - 1
            {
                let prefix_labels = labels[..pos]
                    .iter()
                    .map(|label| label.to_string())
                    .collect();
                let suffix_labels = labels[pos + 1..]
                    .iter()
                    .map(|label| label.to_string())
                    .collect();
                self.simple_internal_wildcards
                    .push((prefix_labels, suffix_labels, index));
                return;
            }
        }

        if single_wildcard_count > 1 && !has_double_star {
            let owned_labels: Vec<String> = labels.iter().map(|label| label.to_string()).collect();
            self.multi_star_trie.insert(&owned_labels, index);
            return;
        }

        if has_double_star {
            let double_star_pos = labels.iter().position(|label| *label == "**");
            if let Some(pos) = double_star_pos
                && pos > 0
                && pos < labels.len() - 1
                && !has_single_star
            {
                let prefix_labels = labels[..pos]
                    .iter()
                    .map(|label| label.to_string())
                    .collect();
                let suffix_labels = labels[pos + 1..]
                    .iter()
                    .map(|label| label.to_string())
                    .collect();
                self.internal_double_star_wildcards
                    .push((prefix_labels, suffix_labels, index));
                return;
            }
        }

        let owned_labels: Vec<String> = pattern.split('.').map(String::from).collect();
        self.complex_patterns.push((owned_labels, index));
    }

    pub fn matches(&self, host: &str) -> Option<usize> {
        if !self.exact_matches.is_empty()
            && let Some(&index) = self.exact_matches.get(host)
        {
            return Some(index);
        }

        if !self.double_star_leading.is_empty() || !self.double_star_trailing.is_empty() {
            let host_labels: Vec<&str> = host.split('.').collect();
            for (&count, known_map) in self.double_star_leading.iter().rev() {
                if host_labels.len() >= count {
                    let candidate = host_labels[host_labels.len() - count..].join(".");
                    if let Some(&index) = known_map.get(&candidate) {
                        return Some(index);
                    }
                }
            }
            for (&count, known_map) in self.double_star_trailing.iter().rev() {
                if host_labels.len() >= count {
                    let candidate = host_labels[..count].join(".");
                    if let Some(&index) = known_map.get(&candidate) {
                        return Some(index);
                    }
                }
            }
        }

        let host_labels: Vec<&str> = host.split('.').collect();

        if !self.simple_internal_wildcards.is_empty() {
            for (prefix_labels, suffix_labels, index) in &self.simple_internal_wildcards {
                if matches_single_internal_wildcard(prefix_labels, suffix_labels, &host_labels) {
                    return Some(*index);
                }
            }
        }

        if !self.internal_double_star_wildcards.is_empty() {
            for (prefix_labels, suffix_labels, index) in &self.internal_double_star_wildcards {
                if matches_internal_double_star(prefix_labels, suffix_labels, &host_labels) {
                    return Some(*index);
                }
            }
        }

        if let Some(index) = self.multi_star_trie.lookup(&host_labels) {
            return Some(index);
        }

        if !self.complex_patterns.is_empty() {
            let result = MATCHES_BUF.with(|buf| {
                let mut matches = buf.borrow_mut();
                for (pattern_labels, index) in &self.complex_patterns {
                    if dp_host_matches(pattern_labels, &host_labels, &mut matches) {
                        return Some(*index);
                    }
                }
                None
            });
            if result.is_some() {
                return result;
            }
        }

        if !self.leading_wildcards.is_empty()
            && let Some((_, rest)) = host.split_once('.')
            && let Some(&index) = self.leading_wildcards.get(rest)
        {
            return Some(index);
        }
        if !self.trailing_wildcards.is_empty()
            && let Some((prefix, _)) = host.rsplit_once('.')
            && let Some(&index) = self.trailing_wildcards.get(prefix)
        {
            return Some(index);
        }
        self.catch_all
    }
}

struct LabelTrie {
    root: TrieNode,
}

struct TrieNode {
    children: HashMap<String, TrieNode>,
    wildcard_child: Option<Box<TrieNode>>,
    value: Option<usize>,
}

impl LabelTrie {
    pub fn new() -> Self {
        Self {
            root: TrieNode::new(),
        }
    }

    pub fn insert(&mut self, pattern_labels: &[String], index: usize) {
        let mut node = &mut self.root;
        for label in pattern_labels {
            if label == "*" {
                node = node
                    .wildcard_child
                    .get_or_insert_with(|| Box::new(TrieNode::new()));
            } else {
                node = node
                    .children
                    .entry(label.clone())
                    .or_insert_with(TrieNode::new);
            }
        }
        if node.value.is_none() {
            node.value = Some(index);
        }
    }

    pub fn lookup(&self, host_labels: &[&str]) -> Option<usize> {
        self.lookup_from(&self.root, host_labels, 0)
    }

    fn lookup_from(&self, node: &TrieNode, host_labels: &[&str], pos: usize) -> Option<usize> {
        if pos == host_labels.len() {
            return node.value;
        }

        let label = host_labels[pos];
        if let Some(child) = node.children.get(label)
            && let Some(found) = self.lookup_from(child, host_labels, pos + 1)
        {
            return Some(found);
        }

        if let Some(wildcard) = node.wildcard_child.as_deref()
            && let Some(found) = self.lookup_from(wildcard, host_labels, pos + 1)
        {
            return Some(found);
        }

        None
    }
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            wildcard_child: None,
            value: None,
        }
    }
}

fn matches_single_internal_wildcard(
    prefix_labels: &[String],
    suffix_labels: &[String],
    host_labels: &[&str],
) -> bool {
    if prefix_labels.len() + 1 + suffix_labels.len() != host_labels.len() {
        return false;
    }

    let prefix_matches = prefix_labels
        .iter()
        .zip(host_labels.iter())
        .all(|(expected, actual)| expected == actual);
    let suffix_matches = suffix_labels
        .iter()
        .zip(host_labels[prefix_labels.len() + 1..].iter())
        .all(|(expected, actual)| expected == actual);

    prefix_matches && suffix_matches
}

fn matches_internal_double_star(
    prefix_labels: &[String],
    suffix_labels: &[String],
    host_labels: &[&str],
) -> bool {
    if prefix_labels.len() + suffix_labels.len() > host_labels.len() {
        return false;
    }

    let start = prefix_labels.len();
    let end = host_labels.len() - suffix_labels.len();
    if start > end {
        return false;
    }

    let prefix_matches = prefix_labels
        .iter()
        .zip(host_labels.iter())
        .all(|(expected, actual)| expected == actual);
    let suffix_matches = suffix_labels
        .iter()
        .zip(host_labels[end..].iter())
        .all(|(expected, actual)| expected == actual);

    prefix_matches && suffix_matches
}

fn dp_host_matches(
    pattern_labels: &[String],
    host_labels: &[&str],
    matches: &mut Vec<bool>,
) -> bool {
    let columns = host_labels.len() + 1;
    let needed = (pattern_labels.len() + 1) * columns;

    matches.clear();
    matches.resize(needed, false);
    matches[0] = true;

    for pattern_pos in 1..=pattern_labels.len() {
        let current_pattern_label = &pattern_labels[pattern_pos - 1];

        for host_pos in 0..=host_labels.len() {
            let index = pattern_pos * columns + host_pos;

            matches[index] = if current_pattern_label == "**" {
                let skip_this_label = matches[(pattern_pos - 1) * columns + host_pos];
                let eat_one_more_label =
                    host_pos > 0 && matches[pattern_pos * columns + (host_pos - 1)];

                skip_this_label || eat_one_more_label
            } else if host_pos > 0 {
                let current_host_label = host_labels[host_pos - 1];
                let previous_labels_matches = matches[(pattern_pos - 1) * columns + (host_pos - 1)];
                let this_label_matches =
                    current_pattern_label == "*" || current_pattern_label == current_host_label;

                previous_labels_matches && this_label_matches
            } else {
                false
            };
        }
    }

    matches[pattern_labels.len() * columns + host_labels.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router_with(patterns: &[(&str, usize)]) -> HostRouter {
        let mut r = HostRouter::new();
        for (pat, idx) in patterns {
            r.add_pattern(pat, *idx);
        }
        r
    }

    #[test]
    fn exact_match() {
        let r = router_with(&[("example.com", 0)]);
        assert_eq!(r.matches("example.com"), Some(0));
    }

    #[test]
    fn exact_no_match() {
        let r = router_with(&[("example.com", 0)]);
        assert_eq!(r.matches("other.com"), None);
    }

    #[test]
    fn leading_wildcard_matches() {
        let r = router_with(&[("*.example.com", 0)]);
        assert_eq!(r.matches("api.example.com"), Some(0));
        assert_eq!(r.matches("www.example.com"), Some(0));
    }

    #[test]
    fn leading_wildcard_no_multilevel() {
        let r = router_with(&[("*.example.com", 0)]);
        assert_eq!(r.matches("v1.api.example.com"), None);
    }

    #[test]
    fn trailing_wildcard_matches() {
        let r = router_with(&[("api.*", 0)]);
        assert_eq!(r.matches("api.com"), Some(0));
        assert_eq!(r.matches("api.io"), Some(0));
    }

    #[test]
    fn double_star_leading_matches_multilevel() {
        let r = router_with(&[("**.example.com", 0)]);
        assert_eq!(r.matches("api.example.com"), Some(0));
        assert_eq!(r.matches("v1.api.example.com"), Some(0));
        assert_eq!(r.matches("a.b.c.example.com"), Some(0));
    }

    #[test]
    fn catch_all_star() {
        let r = router_with(&[("*", 0)]);
        assert_eq!(r.matches("anything.com"), Some(0));
        assert_eq!(r.matches("totally.random.host"), Some(0));
    }

    #[test]
    fn catch_all_double_star() {
        let r = router_with(&[("**", 0)]);
        assert_eq!(r.matches("foo.bar.baz"), Some(0));
    }

    #[test]
    fn exact_beats_wildcard() {
        let r = router_with(&[("*.example.com", 0), ("api.example.com", 1)]);
        assert_eq!(r.matches("api.example.com"), Some(1));
        assert_eq!(r.matches("www.example.com"), Some(0));
    }

    #[test]
    fn complex_pattern_mid_wildcard() {
        let r = router_with(&[("api.*.internal", 0)]);
        assert_eq!(r.matches("api.dev.internal"), Some(0));
        assert_eq!(r.matches("api.prod.internal"), Some(0));
        assert_eq!(r.matches("api.internal"), None);
    }

    #[test]
    fn complex_pattern_no_match() {
        let r = router_with(&[("api.*.internal.**", 0)]);
        assert_eq!(r.matches("api.dev.internal.corp.local"), Some(0));
        assert_eq!(r.matches("other.dev.internal.corp"), None);
    }

    #[test]
    fn simple_internal_wildcard_matches_single_label_between() {
        let r = router_with(&[("example.*.com", 0)]);
        assert_eq!(r.matches("example.foo.com"), Some(0));
        assert_eq!(r.matches("example.bar.com"), Some(0));
        assert_eq!(r.matches("example.com"), None);
        assert_eq!(r.matches("foo.example.com"), None);
    }

    #[test]
    fn complex_internal_double_star_matches_multiple_labels_between() {
        let r = router_with(&[("example.**.com", 0)]);
        assert_eq!(r.matches("example.com"), Some(0));
        assert_eq!(r.matches("example.foo.com"), Some(0));
        assert_eq!(r.matches("example.foo.bar.com"), Some(0));
        assert_eq!(r.matches("foo.example.com"), None);
    }

    #[test]
    fn mixed_star_and_double_star_goes_to_dp() {
        let r = router_with(&[("api.*.v1.**", 0)]);
        assert_eq!(r.matches("api.dev.v1.com"), Some(0));
        assert_eq!(r.matches("api.dev.v1.eu.internal"), Some(0));
        assert_eq!(r.matches("api.v1.com"), None);
    }

    #[test]
    fn multi_star_trie_basic() {
        let r = router_with(&[("a.*.*.c", 0), ("a.b.*.*", 1)]);
        assert_eq!(r.matches("a.b.x.c"), Some(1));
    }

    #[test]
    fn multi_star_trie_backtracking() {
        let r = router_with(&[("x.*.*.*", 0), ("x.y.*.*", 1)]);
        assert_eq!(r.matches("x.y.z.w"), Some(1));
    }

    #[test]
    fn empty_router_returns_none() {
        let r = HostRouter::new();
        assert_eq!(r.matches("example.com"), None);
    }
}
