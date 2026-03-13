use super::{Heap, MAX_ARRAY_LENGTH};
use crate::runtime::Value;
use std::collections::HashMap;

impl Heap {
    pub fn alloc_array(&mut self) -> usize {
        let array_id = self.arrays.len();
        self.arrays.push(Vec::new());
        self.array_props.push(HashMap::new());
        array_id
    }

    #[inline(always)]
    pub fn get_array_prop(&self, array_id: usize, key: &str) -> Value {
        if let Some(elements) = self.arrays.get(array_id) {
            if key.as_bytes() == b"length" {
                return Value::Int(elements.len() as i32);
            }
            if let Ok(index) = key.parse::<usize>() {
                if index < elements.len() {
                    return elements[index].clone();
                }
                if let Some(properties) = self.array_props.get(array_id)
                    && let Some(value) = properties.get(key)
                {
                    return value.clone();
                }
            }
            if let Some(properties) = self.array_props.get(array_id)
                && let Some(value) = properties.get(key)
            {
                return value.clone();
            }
        }
        if let Some(array_prototype_id) = self.array_prototype_id {
            return self.get_prop(array_prototype_id, key);
        }
        Value::Undefined
    }

    pub fn delete_array_prop(&mut self, array_id: usize, key: &str) {
        if key == "length" {
            return;
        }
        if let Ok(index) = key.parse::<usize>() {
            if let Some(elements) = self.arrays.get_mut(array_id)
                && index < elements.len()
            {
                elements[index] = Value::Undefined;
            }
        } else if let Some(properties) = self.array_props.get_mut(array_id) {
            properties.remove(key);
        }
    }

    pub fn set_array_prop(&mut self, array_id: usize, key: &str, value: Value) {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            if key == "length" {
                if let Value::Int(length_value) = value
                    && length_value >= 0
                {
                    let next_length = length_value as usize;
                    elements.truncate(next_length.min(MAX_ARRAY_LENGTH));
                }
                return;
            }
            if let Ok(index) = key.parse::<usize>() {
                if index < MAX_ARRAY_LENGTH {
                    while elements.len() <= index {
                        elements.push(Value::Undefined);
                    }
                    elements[index] = value;
                } else if let Some(properties) = self.array_props.get_mut(array_id) {
                    properties.insert(key.to_string(), value);
                }
            } else if let Some(properties) = self.array_props.get_mut(array_id) {
                properties.insert(key.to_string(), value);
            }
        }
    }

    pub fn array_push(&mut self, array_id: usize, value: Value) {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            if elements.is_empty() {
                elements.reserve(4096);
            }
            elements.push(value);
        }
    }

    pub fn array_push_values(&mut self, array_id: usize, values: &[Value]) -> i32 {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            elements.extend(values.iter().cloned());
            elements.len() as i32
        } else {
            0
        }
    }

    pub fn array_len(&self, array_id: usize) -> usize {
        self.arrays
            .get(array_id)
            .map(|elements| elements.len())
            .unwrap_or(0)
    }

    pub fn array_pop(&mut self, array_id: usize) -> Value {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            elements.pop().unwrap_or(Value::Undefined)
        } else {
            Value::Undefined
        }
    }

    pub fn array_shift(&mut self, array_id: usize) -> Value {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            if elements.is_empty() {
                Value::Undefined
            } else {
                elements.remove(0)
            }
        } else {
            Value::Undefined
        }
    }

    pub fn array_unshift(&mut self, array_id: usize, values: &[Value]) -> i32 {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            for value in values.iter().rev() {
                elements.insert(0, value.clone());
            }
            elements.len() as i32
        } else {
            0
        }
    }

    pub fn array_reverse(&mut self, array_id: usize) {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            elements.reverse();
        }
    }

    pub fn array_fill(&mut self, array_id: usize, value: Value, start: usize, end: usize) {
        if let Some(elements) = self.arrays.get_mut(array_id) {
            let length = elements.len();
            let end_index = end.min(length);
            for index in start..end_index {
                elements[index] = value.clone();
            }
        }
    }

    pub fn array_splice(&mut self, array_id: usize, elements: Vec<Value>) {
        if let Some(array_elements) = self.arrays.get_mut(array_id) {
            *array_elements = elements;
        }
    }

    pub fn array_has_property(&self, array_id: usize, key: &str) -> bool {
        if let Some(elements) = self.arrays.get(array_id) {
            if key == "length" {
                return true;
            }
            if let Ok(index) = key.parse::<usize>()
                && index < elements.len()
            {
                return true;
            }
            if let Some(properties) = self.array_props.get(array_id)
                && properties.contains_key(key)
            {
                return true;
            }
        }
        if let Some(array_prototype_id) = self.array_prototype_id {
            return self.object_has_property(array_prototype_id, key);
        }
        false
    }

    pub fn array_has_own_property(&self, array_id: usize, key: &str) -> bool {
        if key == "length" {
            return true;
        }
        if let Some(elements) = self.arrays.get(array_id)
            && let Ok(index) = key.parse::<usize>()
        {
            return index < elements.len();
        }
        self.array_props
            .get(array_id)
            .map(|properties| properties.contains_key(key))
            .unwrap_or(false)
    }

    pub fn array_elements(&self, array_id: usize) -> Option<&[Value]> {
        self.arrays
            .get(array_id)
            .map(|elements| elements.as_slice())
    }
}
