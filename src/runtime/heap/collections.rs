use super::Heap;
use crate::runtime::Value;

impl Heap {
    pub fn alloc_map(&mut self) -> usize {
        let map_id = self.maps.len();
        self.maps.push(std::collections::HashMap::new());
        map_id
    }

    pub fn map_set(&mut self, map_id: usize, key: &str, value: Value) {
        if let Some(map) = self.maps.get_mut(map_id) {
            map.insert(key.to_string(), value);
        }
    }

    pub fn map_get(&self, map_id: usize, key: &str) -> Value {
        self.maps
            .get(map_id)
            .and_then(|map| map.get(key).cloned())
            .unwrap_or(Value::Undefined)
    }

    pub fn map_has(&self, map_id: usize, key: &str) -> bool {
        self.maps
            .get(map_id)
            .map(|map| map.contains_key(key))
            .unwrap_or(false)
    }

    pub fn map_size(&self, map_id: usize) -> usize {
        self.maps.get(map_id).map(|map| map.len()).unwrap_or(0)
    }

    pub fn map_keys(&self, map_id: usize) -> Vec<String> {
        self.maps
            .get(map_id)
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn alloc_set(&mut self) -> usize {
        let set_id = self.sets.len();
        self.sets.push(std::collections::HashSet::new());
        set_id
    }

    pub fn set_add(&mut self, set_id: usize, key: &str) {
        if let Some(set) = self.sets.get_mut(set_id) {
            set.insert(key.to_string());
        }
    }

    pub fn set_has(&self, set_id: usize, key: &str) -> bool {
        self.sets
            .get(set_id)
            .map(|set| set.contains(key))
            .unwrap_or(false)
    }

    pub fn set_size(&self, set_id: usize) -> usize {
        self.sets.get(set_id).map(|set| set.len()).unwrap_or(0)
    }

    pub fn set_keys(&self, set_id: usize) -> Vec<String> {
        self.sets
            .get(set_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn alloc_date(&mut self, timestamp_ms: f64) -> usize {
        let date_id = self.dates.len();
        self.dates.push(timestamp_ms);
        date_id
    }

    pub fn date_timestamp(&self, date_id: usize) -> f64 {
        self.dates.get(date_id).copied().unwrap_or(0.0)
    }

    pub fn set_date_timestamp(&mut self, date_id: usize, milliseconds: f64) {
        if let Some(date_slot) = self.dates.get_mut(date_id) {
            *date_slot = milliseconds;
        }
    }
}
