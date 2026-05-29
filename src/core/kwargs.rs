use std::collections::HashMap;
use std::borrow::Cow;

#[derive(Debug)]
pub struct Kwargs<'a> {
    map: HashMap<&'a str, Cow<'a, str>>,
}

impl<'a> Kwargs<'a> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn set<V>(&mut self, key: &'a str, value: V)
    where
        V: Into<Cow<'a, str>>,
    {
        self.map.insert(key, value.into());
    }

    pub fn get(&self, key: &str, default: &'a str) -> &str {
        self.map
            .get(key)
            .map(|v| v.as_ref())
            .unwrap_or(default)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&&'a str, &Cow<'a, str>)> + '_ {
        self.map.iter()
    }
}