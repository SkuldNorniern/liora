use super::Heap;
use crate::runtime::Value;
use crate::runtime::builtins::internal;

impl Heap {
    pub fn is_html_dda_object(&self, object_id: usize) -> bool {
        self.is_html_dda_object_id == Some(object_id)
    }

    pub fn typed_array_constructor_value(&self) -> Value {
        self.typed_array_constructor_id
            .map(Value::Object)
            .unwrap_or(Value::Undefined)
    }

    pub fn array_buffer_prototype_value(&self) -> Value {
        self.array_buffer_prototype_id
            .map(Value::Object)
            .unwrap_or(Value::Undefined)
    }

    pub fn typed_array_prototype_value(&self) -> Value {
        self.typed_array_prototype_id
            .map(Value::Object)
            .unwrap_or(Value::Undefined)
    }

    pub fn get_global(&self, name: &str) -> Value {
        self.get_prop(self.global_object_id, name)
    }

    pub fn global_object(&self) -> usize {
        self.global_object_id
    }

    pub fn alloc_object(&mut self) -> usize {
        self.alloc_object_with_prototype(None)
    }

    pub fn alloc_arguments_object(&mut self, args: &[Value], callee: Option<Value>) -> usize {
        let prototype = match self.get_global("Object") {
            Value::Object(object_constructor_id) => {
                match self.get_prop(object_constructor_id, "prototype") {
                    Value::Object(object_prototype_id) => Some(object_prototype_id),
                    _ => None,
                }
            }
            _ => None,
        };
        let arguments_object_id = self.alloc_object_with_prototype(prototype);
        for (index, value) in args.iter().enumerate() {
            self.set_prop(arguments_object_id, &index.to_string(), value.clone());
        }
        self.set_prop(
            arguments_object_id,
            internal::ARGUMENTS_LENGTH_PROPERTY,
            Value::Int(args.len() as i32),
        );
        if let Some(callee_value) = callee {
            self.set_prop(
                arguments_object_id,
                internal::ARGUMENTS_CALLEE_PROPERTY,
                callee_value,
            );
        }
        self.set_prop(
            arguments_object_id,
            internal::ARGUMENTS_OBJECT_MARKER,
            Value::Bool(true),
        );
        arguments_object_id
    }

    pub fn alloc_proxy_object(&mut self, target: Value, handler: Value) -> usize {
        let proxy_object_id = self.alloc_object();
        self.set_prop(
            proxy_object_id,
            internal::PROXY_OBJECT_MARKER,
            Value::Bool(true),
        );
        self.set_prop(proxy_object_id, internal::PROXY_TARGET_VALUE, target);
        self.set_prop(proxy_object_id, internal::PROXY_HANDLER_VALUE, handler);
        proxy_object_id
    }

    pub fn is_proxy_object(&self, object_id: usize) -> bool {
        internal::is_proxy_object(self, object_id)
    }

    pub fn proxy_target_value(&self, proxy_object_id: usize) -> Option<Value> {
        if !self.is_proxy_object(proxy_object_id) {
            return None;
        }
        let value = self.get_prop(proxy_object_id, internal::PROXY_TARGET_VALUE);
        if matches!(value, Value::Undefined) {
            None
        } else {
            Some(value)
        }
    }

    pub fn proxy_handler_value(&self, proxy_object_id: usize) -> Option<Value> {
        if !self.is_proxy_object(proxy_object_id) {
            return None;
        }
        let value = self.get_prop(proxy_object_id, internal::PROXY_HANDLER_VALUE);
        if matches!(value, Value::Undefined) {
            None
        } else {
            Some(value)
        }
    }

    pub fn alloc_regexp(&mut self) -> usize {
        self.alloc_object_with_prototype(self.regexp_prototype_id)
    }

    pub fn alloc_object_with_prototype(&mut self, prototype: Option<usize>) -> usize {
        let id = self.objects.len();
        self.objects.push(super::HeapObject {
            props: std::collections::HashMap::new(),
            prototype,
        });
        id
    }

    pub fn get_proto(&self, object_id: usize) -> Option<usize> {
        self.objects
            .get(object_id)
            .and_then(|object| object.prototype)
    }

    pub fn set_prototype(&mut self, object_id: usize, prototype: Option<usize>) {
        if self.objects.get(object_id).is_none() {
            return;
        }
        if let Some(mut current_prototype_id) = prototype {
            let mut traversed = 0usize;
            while traversed <= self.objects.len() {
                if current_prototype_id == object_id {
                    return;
                }
                let Some(next_prototype_id) = self.get_proto(current_prototype_id) else {
                    break;
                };
                current_prototype_id = next_prototype_id;
                traversed += 1;
            }
        }
        if let Some(object) = self.objects.get_mut(object_id) {
            object.prototype = prototype;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_prototype_rejects_cycle() {
        let mut heap = Heap::new();
        let object_a = heap.alloc_object();
        let object_b = heap.alloc_object();

        heap.set_prototype(object_a, Some(object_b));
        assert_eq!(heap.get_proto(object_a), Some(object_b));

        heap.set_prototype(object_b, Some(object_a));
        assert_eq!(heap.get_proto(object_b), None);
    }
}
