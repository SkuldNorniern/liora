use super::Heap;
use crate::runtime::Value;

impl Heap {
    #[inline(always)]
    pub fn get_prop(&self, object_id: usize, key: &str) -> Value {
        let mut current_object_id = Some(object_id);
        while let Some(id) = current_object_id {
            if let Some(object) = self.objects.get(id) {
                if let Some(value) = object.props.get(key) {
                    return value.clone();
                }
                current_object_id = object.prototype;
            } else {
                break;
            }
        }
        Value::Undefined
    }

    pub fn set_prop(&mut self, object_id: usize, key: &str, value: Value) {
        if let Some(object) = self.objects.get_mut(object_id) {
            object.props.insert(key.to_string(), value);
        }
    }

    pub fn delete_prop(&mut self, object_id: usize, key: &str) -> bool {
        if let Some(object) = self.objects.get_mut(object_id) {
            object.props.remove(key).is_some()
        } else {
            false
        }
    }

    pub fn object_has_own_property(&self, object_id: usize, key: &str) -> bool {
        self.objects
            .get(object_id)
            .map(|object| object.props.contains_key(key))
            .unwrap_or(false)
    }

    pub fn object_has_property(&self, object_id: usize, key: &str) -> bool {
        let mut current_object_id = Some(object_id);
        while let Some(id) = current_object_id {
            if let Some(object) = self.objects.get(id) {
                if object.props.contains_key(key) {
                    return true;
                }
                current_object_id = object.prototype;
            } else {
                break;
            }
        }
        false
    }

    pub fn object_keys(&self, object_id: usize) -> Vec<String> {
        self.objects
            .get(object_id)
            .map(|object| {
                object
                    .props
                    .iter()
                    .filter_map(|(key, value)| {
                        if key.starts_with("__") || matches!(value, Value::Builtin(_)) {
                            None
                        } else {
                            Some(key.clone())
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn object_property_names(&self, object_id: usize) -> Vec<String> {
        self.objects
            .get(object_id)
            .map(|object| {
                object
                    .props
                    .keys()
                    .filter(|key| !key.starts_with("__"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}
