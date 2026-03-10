use crate::runtime::Heap;

pub const INTERNAL_PROPERTY_PREFIX: &str = "__liora_";
pub const ARGUMENTS_OBJECT_MARKER: &str = "__liora_arguments_object__";
pub const ARGUMENTS_LENGTH_PROPERTY: &str = "length";
pub const ARGUMENTS_CALLEE_PROPERTY: &str = "callee";
pub const PROXY_OBJECT_MARKER: &str = "__liora_proxy_object__";
pub const PROXY_TARGET_VALUE: &str = "__liora_proxy_target__";
pub const PROXY_HANDLER_VALUE: &str = "__liora_proxy_handler__";

pub fn is_internal_property_name(name: &str) -> bool {
    name.starts_with(INTERNAL_PROPERTY_PREFIX)
}

pub fn is_arguments_object(heap: &Heap, object_id: usize) -> bool {
    heap.object_has_own_property(object_id, ARGUMENTS_OBJECT_MARKER)
}

pub fn is_arguments_non_enumerable_property(name: &str) -> bool {
    name == ARGUMENTS_LENGTH_PROPERTY || name == ARGUMENTS_CALLEE_PROPERTY
}

pub fn should_hide_from_object_keys(heap: &Heap, object_id: usize, key: &str) -> bool {
    is_internal_property_name(key)
        || (is_arguments_object(heap, object_id) && is_arguments_non_enumerable_property(key))
}

pub fn is_proxy_object(heap: &Heap, object_id: usize) -> bool {
    heap.object_has_own_property(object_id, PROXY_OBJECT_MARKER)
}
