#![allow(unused)]
use std::collections::HashMap;
use tracing::warn;

pub struct HostRouter {
    exact_matches: HashMap<String, usize>,
    leading_wildcards: HashMap<String, usize>,
    trailing_wildcards: HashMap<String, usize>,
    double_star_leading: HashMap<usize, HashMap<String, usize>>,
    double_star_trailing: HashMap<usize, HashMap<String, usize>>,
    complex_patterns: Vec<(String, usize)>,
    catch_all: Option<usize>,
}

impl HostRouter {
    pub fn new() -> Self {
        Self {
            exact_matches: HashMap::new(),
            leading_wildcards: HashMap::new(),
            trailing_wildcards: HashMap::new(),
            double_star_leading: HashMap::new(),
            double_star_trailing: HashMap::new(),
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

        self.complex_patterns.push((pattern.to_string(), index));
    }

    pub fn matches(&self, host: &str) -> Option<usize> {
        if !self.exact_matches.is_empty()
            && let Some(&index) = self.exact_matches.get(host)
        {
            return Some(index);
        }
        if !self.double_star_leading.is_empty() || !self.double_star_trailing.is_empty() {
            let host_labels: Vec<&str> = host.split('.').collect();
            for (&count, known_map) in &self.double_star_leading {
                if host_labels.len() >= count {
                    let candidate = host_labels[host_labels.len() - count..].join(".");
                    if let Some(&index) = known_map.get(&candidate) {
                        return Some(index);
                    }
                }
            }
            for (&count, known_map) in &self.double_star_trailing {
                if host_labels.len() >= count {
                    let candidate = host_labels[..count].join(".");
                    if let Some(&index) = known_map.get(&candidate) {
                        return Some(index);
                    }
                }
            }
        }
        if !self.complex_patterns.is_empty() {
            for (pattern, index) in &self.complex_patterns {
                if dp_host_matches(pattern, host) {
                    return Some(*index);
                }
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

fn dp_host_matches(pattern: &str, host: &str) -> bool {
    let pattern_labels: Vec<&str> = pattern.split('.').collect();
    let host_labels: Vec<&str> = host.split('.').collect();

    let columns = host_labels.len() + 1;

    let mut matches = vec![false; (pattern_labels.len() + 1) * columns];
    matches[0] = true;

    for pattern_pos in 1..=pattern_labels.len() {
        let current_pattern_label = pattern_labels[pattern_pos - 1];

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
