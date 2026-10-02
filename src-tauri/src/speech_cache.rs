//! Bounded session-only LRU. Keys are digests; no text/audio is persisted.
use std::collections::VecDeque;

pub struct AudioCache<T> {
    items: VecDeque<(String, T, usize)>,
    bytes: usize,
    budget: usize,
}
impl<T: Clone> AudioCache<T> {
    pub fn new(budget: usize) -> Self {
        Self {
            items: VecDeque::new(),
            bytes: 0,
            budget,
        }
    }
    pub fn get(&mut self, key: &str) -> Option<T> {
        let index = self.items.iter().position(|item| item.0 == key)?;
        let item = self.items.remove(index)?;
        let value = item.1.clone();
        self.items.push_back(item);
        Some(value)
    }
    pub fn insert(&mut self, key: String, value: T, size: usize) {
        if size > self.budget {
            return;
        }
        if let Some(index) = self.items.iter().position(|item| item.0 == key) {
            self.bytes -= self.items.remove(index).unwrap().2;
        }
        while self.bytes + size > self.budget || self.items.len() >= 64 {
            if let Some(item) = self.items.pop_front() {
                self.bytes -= item.2;
            } else {
                break;
            }
        }
        self.bytes += size;
        self.items.push_back((key, value, size));
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lru_is_bounded_and_promotes_hits() {
        let mut cache = AudioCache::new(6);
        cache.insert("a".into(), 1, 3);
        cache.insert("b".into(), 2, 3);
        assert_eq!(cache.get("a"), Some(1));
        cache.insert("c".into(), 3, 3);
        assert_eq!(cache.get("b"), None);
        assert_eq!(cache.get("a"), Some(1));
        cache.insert("huge".into(), 4, 100);
        assert_eq!(cache.get("huge"), None);
        cache.clear();
        assert_eq!(cache.get("a"), None);
    }
}
