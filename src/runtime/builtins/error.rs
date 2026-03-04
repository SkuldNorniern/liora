use crate::runtime::{Heap, Value};

fn make_error(heap: &mut Heap, msg: String, constructor_name: &str) -> Value {
    let obj_id = heap.alloc_object();
    heap.record_error_object(obj_id);
    heap.set_prop(obj_id, "message", Value::String(msg));
    heap.set_prop(obj_id, "name", Value::String(constructor_name.to_string()));
    let constructor = heap.get_global(constructor_name);
    heap.set_prop(obj_id, "constructor", constructor);
    Value::Object(obj_id)
}

pub fn error(args: &[Value], heap: &mut Heap) -> Value {
    make_error(heap, error_message_arg(args), "Error")
}

fn error_message_arg(args: &[Value]) -> String {
    args.first()
        .filter(|v| !matches!(v, Value::Undefined | Value::Null))
        .map(|v| v.to_string())
        .unwrap_or_default()
}

pub fn reference_error(args: &[Value], heap: &mut Heap) -> Value {
    make_error(heap, error_message_arg(args), "ReferenceError")
}

pub fn type_error(args: &[Value], heap: &mut Heap) -> Value {
    make_error(heap, error_message_arg(args), "TypeError")
}

pub fn range_error(args: &[Value], heap: &mut Heap) -> Value {
    make_error(heap, error_message_arg(args), "RangeError")
}

pub fn syntax_error(args: &[Value], heap: &mut Heap) -> Value {
    make_error(heap, error_message_arg(args), "SyntaxError")
}

pub fn uri_error(args: &[Value], heap: &mut Heap) -> Value {
    make_error(heap, error_message_arg(args), "URIError")
}

pub fn is_error(args: &[Value], heap: &mut Heap) -> Value {
    let result = match args.first() {
        Some(Value::Object(id)) => heap.is_error_object(*id),
        _ => false,
    };
    Value::Bool(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_constructor_sets_name_and_message() {
        let mut heap = Heap::new();
        let v = error(&[Value::String("msg".to_string())], &mut heap);
        if let Value::Object(id) = v {
            let name = heap.get_prop(id, "name");
            let msg = heap.get_prop(id, "message");
            assert!(matches!(name, Value::String(s) if s == "Error"));
            assert!(matches!(msg, Value::String(s) if s == "msg"));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn reference_error_sets_constructor() {
        let mut heap = Heap::new();
        let v = reference_error(&[Value::String("x".to_string())], &mut heap);
        if let Value::Object(id) = v {
            let ctor = heap.get_prop(id, "constructor");
            let ref_err = heap.get_global("ReferenceError");
            assert_eq!(ctor, ref_err);
        } else {
            panic!("expected Object");
        }
    }
}
