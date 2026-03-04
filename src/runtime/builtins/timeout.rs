//! timeout(callback, delay) - test262 host hook stub.
//!
//! test262 harness scripts may call this helper to schedule asynchronous cleanup.
//! For jsina's synchronous harness mode we currently treat it as a no-op.

use super::{BuiltinContext, BuiltinError};
use crate::runtime::Value;

pub fn timeout(args: &[Value], _ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let _ = args;
    Ok(Value::Undefined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn timeout_is_noop() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let r = timeout(&[], &mut ctx);
        assert!(matches!(r, Ok(Value::Undefined)));
    }
}
