use super::Heap;
use crate::runtime::Value;

impl Heap {
    pub fn delete_builtin_prop(&mut self, builtin_id: u8, key: &str) {
        self.deleted_builtin_props
            .entry(builtin_id)
            .or_default()
            .insert(key.to_string());
    }

    pub fn builtin_prop_deleted(&self, builtin_id: u8, key: &str) -> bool {
        self.deleted_builtin_props
            .get(&builtin_id)
            .is_some_and(|deleted| deleted.contains(key))
    }

    pub fn record_error_object(&mut self, object_id: usize) {
        self.error_object_ids.insert(object_id);
    }

    pub fn is_error_object(&self, object_id: usize) -> bool {
        self.error_object_ids.contains(&object_id)
    }

    pub fn format_thrown_value(&self, value: &Value) -> String {
        match value {
            Value::Object(object_id) => {
                let name = match self.get_prop(*object_id, "name") {
                    Value::String(name) => name,
                    _ => String::new(),
                };
                let name_string = if name.is_empty() {
                    if let Value::Object(constructor_id) = self.get_prop(*object_id, "constructor")
                    {
                        if let Value::String(constructor_name) =
                            self.get_prop(constructor_id, "name")
                        {
                            if !constructor_name.is_empty() {
                                constructor_name
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        }
                    } else if self.is_error_object(*object_id) {
                        "Error".to_string()
                    } else {
                        String::new()
                    }
                } else {
                    name.clone()
                };
                let message_value = self.get_prop(*object_id, "message");
                let message = match &message_value {
                    Value::String(message_text) => {
                        if message_text == "undefined" {
                            String::new()
                        } else {
                            message_text.clone()
                        }
                    }
                    Value::Undefined | Value::Null => String::new(),
                    _ => message_value.to_string(),
                };
                if self.is_error_object(*object_id)
                    || !name_string.is_empty()
                    || !message.is_empty()
                {
                    if message.is_empty() {
                        name_string
                    } else {
                        format!("{}: {}", name_string, message)
                    }
                } else {
                    let constructor = self.get_prop(*object_id, "constructor");
                    if let Value::Object(constructor_id) = constructor {
                        if let Value::String(constructor_name) =
                            self.get_prop(constructor_id, "name")
                        {
                            if !constructor_name.is_empty() {
                                format!("[object {}]", constructor_name)
                            } else {
                                "[object Object]".to_string()
                            }
                        } else {
                            "[object Object]".to_string()
                        }
                    } else {
                        "[object Object]".to_string()
                    }
                }
            }
            Value::Undefined => "thrown undefined".to_string(),
            _ => value.to_string(),
        }
    }
}
