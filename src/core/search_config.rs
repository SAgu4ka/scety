#![allow(unused)]
use std::collections::HashSet;

pub struct HostRouter {
    exact_matches: HashSet<String>,
    leading_wildcards: HashSet<String>,
    trailing_wildcards: HashSet<String>,
    complex_patterns: Vec<String>,
    catch_all: bool,
}

impl HostRouter {
    pub fn new() -> Self {
        Self {
            exact_matches: HashSet::new(),
            leading_wildcards: HashSet::new(),
            trailing_wildcards: HashSet::new(),
            complex_patterns: Vec::new(),
            catch_all: false,
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) {
        if pattern == "*" || pattern == "**" {
            // The simple symbols "*" and "**" mean a match for all domains.
            self.catch_all = true;
            return;
        }

        let star_count = pattern.matches('*').count();

        if star_count == 0 {
            self.exact_matches.insert(pattern.to_string());
        } else if star_count == 1 && pattern.starts_with("*.") {
            let domain_part = &pattern[2..];
            self.leading_wildcards.insert(domain_part.to_string());
        } else if star_count == 1 && pattern.ends_with(".*") {
            let domain_part = &pattern[..pattern.len() - 2];
            self.trailing_wildcards.insert(domain_part.to_string());
        } else {
            self.complex_patterns.push(pattern.to_string());
        }
    }

    pub fn matches(&self, host: &str) -> bool {
        if self.catch_all {
            return true;
        }

        if self.exact_matches.contains(host) {
            return true;
        }

        if let Some((_, rest)) = host.split_once('.')
            && self.leading_wildcards.contains(rest)
        {
            return true;
        }

        if let Some((prefix, _)) = host.rsplit_once('.')
            && self.trailing_wildcards.contains(prefix)
        {
            return true;
        }

        for pattern in &self.complex_patterns {
            if dp_host_matches(pattern, host) {
                return true;
            }
        }

        false
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
