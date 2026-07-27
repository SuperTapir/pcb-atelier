use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::Arc,
};

#[derive(Debug)]
struct Entry<V> {
    value: Arc<V>,
    estimated_bytes: usize,
}

#[derive(Debug)]
pub struct ByteBudgetLru<K, V> {
    budget_bytes: usize,
    resident_bytes: usize,
    entries: HashMap<K, Entry<V>>,
    recency: VecDeque<K>,
}

impl<K, V> ByteBudgetLru<K, V>
where
    K: Clone + Eq + Hash,
{
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            resident_bytes: 0,
            entries: HashMap::new(),
            recency: VecDeque::new(),
        }
    }

    pub const fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    pub const fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&mut self, key: &K) -> Option<Arc<V>> {
        let value = Arc::clone(&self.entries.get(key)?.value);
        self.touch(key);
        Some(value)
    }

    pub fn insert(&mut self, key: K, value: Arc<V>, estimated_bytes: usize) -> bool {
        if estimated_bytes > self.budget_bytes {
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(previous.estimated_bytes);
            self.recency.retain(|candidate| candidate != &key);
        }
        while self.resident_bytes.saturating_add(estimated_bytes) > self.budget_bytes {
            let Some(oldest) = self.recency.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.resident_bytes = self.resident_bytes.saturating_sub(removed.estimated_bytes);
            }
        }
        self.resident_bytes = self.resident_bytes.saturating_add(estimated_bytes);
        self.recency.push_back(key.clone());
        self.entries.insert(
            key,
            Entry {
                value,
                estimated_bytes,
            },
        );
        true
    }

    pub fn remove(&mut self, key: &K) -> Option<Arc<V>> {
        let removed = self.entries.remove(key)?;
        self.resident_bytes = self.resident_bytes.saturating_sub(removed.estimated_bytes);
        self.recency.retain(|candidate| candidate != key);
        Some(removed.value)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
        self.resident_bytes = 0;
    }

    fn touch(&mut self, key: &K) {
        self.recency.retain(|candidate| candidate != key);
        self.recency.push_back(key.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ByteBudgetLru;

    #[test]
    fn evicts_by_estimated_bytes_and_keeps_external_references_alive() {
        let mut cache = ByteBudgetLru::new(10);
        let first = Arc::new(vec![1_u8; 6]);
        assert!(cache.insert("first", Arc::clone(&first), 6));
        assert!(cache.insert("second", Arc::new(vec![2_u8; 6]), 6));
        assert!(cache.get(&"first").is_none());
        assert_eq!(first.len(), 6, "active session reference remains valid");
        assert_eq!(cache.resident_bytes(), 6);
    }

    #[test]
    fn recent_access_controls_eviction_and_oversized_values_are_rejected() {
        let mut cache = ByteBudgetLru::new(10);
        assert!(cache.insert("a", Arc::new(1), 4));
        assert!(cache.insert("b", Arc::new(2), 4));
        assert_eq!(*cache.get(&"a").expect("touch a"), 1);
        assert!(cache.insert("c", Arc::new(3), 4));
        assert!(cache.get(&"b").is_none());
        assert!(cache.get(&"a").is_some());
        assert!(!cache.insert("large", Arc::new(4), 11));
    }
}
