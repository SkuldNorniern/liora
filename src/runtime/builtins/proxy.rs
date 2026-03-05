//! Proxy builtin. Stub: returns target (transparent proxy). Handler traps not implemented.
//! Sufficient for libraries that check `typeof Proxy !== "undefined"` and basic usage.

use crate::runtime::Value;

pub fn create(args: &[Value], _heap: &mut crate::runtime::Heap) -> Value {
    args.first()
        .cloned()
        .unwrap_or(Value::Null)
}
