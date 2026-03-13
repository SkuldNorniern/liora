use super::Heap;

impl Heap {
    pub fn symbol_for(&mut self, key: &str) -> usize {
        if let Some(&symbol_id) = self.symbol_for_registry.get(key) {
            return symbol_id;
        }
        let symbol_id = self.alloc_symbol(Some(key.to_string()));
        self.symbol_for_registry.insert(key.to_string(), symbol_id);
        symbol_id
    }

    pub fn symbol_key_for(&self, symbol_id: usize) -> Option<&str> {
        self.symbol_for_registry
            .iter()
            .find(|(_, value)| **value == symbol_id)
            .map(|(key, _)| key.as_str())
    }

    pub fn alloc_symbol(&mut self, description: Option<String>) -> usize {
        let symbol_id = self.symbols.len();
        self.symbols.push(description);
        symbol_id
    }

    pub fn symbol_description(&self, symbol_id: usize) -> Option<&str> {
        self.symbols
            .get(symbol_id)
            .and_then(|description| description.as_deref())
    }
}
