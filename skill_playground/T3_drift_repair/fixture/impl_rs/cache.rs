use std::collections::HashMap;

pub struct Cache {
    map: HashMap<i64, i64>,
    cap: usize,
}

impl Cache {
    pub fn new(cap: usize) -> Self { Cache { map: HashMap::new(), cap } }
    pub fn get(&self, k: i64) -> Option<&i64> { self.map.get(&k) }
    pub fn put(&mut self, k: i64, v: i64) {
        if self.map.len() >= self.cap { /* drop something — placeholder */ }
        self.map.insert(k, v);
    }
    pub fn size(&self) -> usize { self.map.len() }
}
