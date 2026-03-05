//! test262 $262 host object stubs. Minimal implementation for harness-dependent tests.

use super::BuiltinContext;
use crate::runtime::builtins;
use crate::runtime::Value;

pub fn create_realm(
    _args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, super::BuiltinError> {
    let heap = &mut ctx.heap;
    let global_id = heap.global_object();
    let eval_id = builtins::resolve("Global", "eval").expect("eval builtin must exist");
    let realm_id = heap.alloc_object();
    heap.set_prop(realm_id, "global", Value::Object(global_id));
    heap.set_prop(realm_id, "evalScript", Value::Builtin(eval_id));
    Ok(Value::Object(realm_id))
}

pub fn eval_script(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, super::BuiltinError> {
    super::eval::eval(args, ctx)
}

pub fn gc(_args: &[Value], _ctx: &mut BuiltinContext) -> Result<Value, super::BuiltinError> {
    Ok(Value::Undefined)
}

pub fn detach_array_buffer(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, super::BuiltinError> {
    let heap = &mut ctx.heap;
    let buffer = args.first().and_then(|v| v.as_object_id());
    if let Some(id) = buffer {
        heap.set_prop(id, "byteLength", Value::Int(0));
    }
    Ok(Value::Undefined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn gc_is_noop() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let args = [];
        let result = gc(&args, &mut ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Undefined);
    }

    #[test]
    fn eval_script_delegates_to_eval() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let args = [Value::String("return 1 + 2".to_string())];
        let result = eval_script(&args, &mut ctx);
        assert!(result.is_ok());
        let v = result.unwrap();
        assert_eq!(v, Value::Int(3));
    }
}
