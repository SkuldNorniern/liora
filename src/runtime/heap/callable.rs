use super::Heap;
use crate::runtime::Value;
use std::collections::HashMap;

impl Heap {
    pub fn set_eval_scope_bindings(&mut self, bindings: Vec<(String, Value)>) {
        self.eval_scope_bindings = bindings;
    }

    pub fn clear_eval_scope_bindings(&mut self) {
        self.eval_scope_bindings.clear();
    }

    pub fn eval_scope_bindings(&self) -> Vec<(String, Value)> {
        self.eval_scope_bindings.clone()
    }

    pub fn get_function_prop(&self, func_index: usize, key: &str) -> Value {
        self.function_props
            .get(&func_index)
            .and_then(|properties| properties.get(key).cloned())
            .unwrap_or(Value::Undefined)
    }

    pub fn set_function_prop(&mut self, func_index: usize, key: &str, value: Value) {
        let properties = self.function_props.entry(func_index).or_default();
        if key == "name" && properties.contains_key("name") {
            return;
        }
        properties.insert(key.to_string(), value);
    }

    pub fn ensure_function_prototype(&mut self, func_index: usize) -> usize {
        if let Value::Object(prototype_id) = self.get_function_prop(func_index, "prototype") {
            return prototype_id;
        }
        let prototype_id = self.alloc_object();
        self.set_prop(prototype_id, "constructor", Value::Function(func_index));
        self.set_function_prop(func_index, "prototype", Value::Object(prototype_id));
        prototype_id
    }

    pub fn get_dynamic_function_prop(&self, dynamic_function_index: usize, key: &str) -> Value {
        self.dynamic_function_props
            .get(dynamic_function_index)
            .and_then(|properties| properties.get(key).cloned())
            .unwrap_or(Value::Undefined)
    }

    pub fn set_dynamic_function_prop(
        &mut self,
        dynamic_function_index: usize,
        key: &str,
        value: Value,
    ) {
        if dynamic_function_index >= self.dynamic_function_props.len() {
            self.dynamic_function_props
                .resize_with(dynamic_function_index + 1, HashMap::new);
        }
        self.dynamic_function_props[dynamic_function_index].insert(key.to_string(), value);
    }

    pub fn ensure_dynamic_function_prototype(&mut self, dynamic_function_index: usize) -> usize {
        if let Value::Object(prototype_id) =
            self.get_dynamic_function_prop(dynamic_function_index, "prototype")
        {
            return prototype_id;
        }
        let prototype_id = self.alloc_object();
        self.set_prop(
            prototype_id,
            "constructor",
            Value::DynamicFunction(dynamic_function_index),
        );
        self.set_dynamic_function_prop(
            dynamic_function_index,
            "prototype",
            Value::Object(prototype_id),
        );
        prototype_id
    }

    pub fn function_has_own_property(&self, function_index: usize, key: &str) -> bool {
        self.function_props
            .get(&function_index)
            .map(|properties| properties.contains_key(key))
            .unwrap_or(false)
    }

    pub fn function_keys(&self, function_index: usize) -> Vec<String> {
        self.function_props
            .get(&function_index)
            .map(|properties| properties.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn delete_function_prop(&mut self, function_index: usize, key: &str) {
        if let Some(properties) = self.function_props.get_mut(&function_index) {
            properties.remove(key);
        }
    }
}
