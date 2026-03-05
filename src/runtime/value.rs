#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Int(i32),
    Number(f64),
    BigInt(String),
    String(String),
    Symbol(usize),
    Object(usize),
    Array(usize),
    Map(usize),
    Set(usize),
    Date(usize),
    Function(usize),
    DynamicFunction(usize),
    Builtin(u8),
    /// (builtin_id, bound_value, binds_function_target)
    /// binds_function_target=true: call.bind(target) binds Function.prototype.call target
    /// binds_function_target=false: method.bind(this_val) binds normal this receiver
    BoundBuiltin(u8, Box<Value>, bool),
    /// (target, bound_this, bound_args) for Function/DynamicFunction.bind()
    BoundFunction(Box<Value>, Box<Value>, Vec<Value>),
    /// Index into heap.generator_states
    Generator(usize),
    /// Index into heap.promises
    Promise(usize),
}

impl Value {
    pub fn to_i64(&self) -> i64 {
        match self {
            Value::Int(n) => *n as i64,
            Value::Number(n) => *n as i64,
            Value::BigInt(s) => s.parse().unwrap_or(0),
            Value::Bool(b) => i64::from(*b),
            _ => 0,
        }
    }

    pub fn to_i32(&self) -> i32 {
        self.to_i64() as i32
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    pub fn is_map(&self) -> bool {
        matches!(self, Value::Map(_))
    }

    pub fn is_set(&self) -> bool {
        matches!(self, Value::Set(_))
    }

    pub fn as_object_id(&self) -> Option<usize> {
        match self {
            Value::Object(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_array_id(&self) -> Option<usize> {
        match self {
            Value::Array(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_map_id(&self) -> Option<usize> {
        match self {
            Value::Map(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_set_id(&self) -> Option<usize> {
        match self {
            Value::Set(id) => Some(*id),
            _ => None,
        }
    }

    pub fn is_date(&self) -> bool {
        matches!(self, Value::Date(_))
    }

    pub fn as_date_id(&self) -> Option<usize> {
        match self {
            Value::Date(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_symbol_id(&self) -> Option<usize> {
        match self {
            Value::Symbol(id) => Some(*id),
            _ => None,
        }
    }

    pub fn type_name_for_error(&self) -> &'static str {
        match self {
            Value::Undefined => "undefined",
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Int(_) | Value::Number(_) => "number",
            Value::BigInt(_) => "bigint",
            Value::String(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            Value::Map(_) => "map",
            Value::Set(_) => "set",
            Value::Date(_) => "date",
            Value::Function(_)
            | Value::DynamicFunction(_)
            | Value::Builtin(_)
            | Value::BoundBuiltin(_, _, _)
            | Value::BoundFunction(_, _, _) => "function",
            Value::Generator(_) => "object",
            Value::Promise(_) => "object",
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Undefined => write!(f, "undefined"),
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(n) => write!(f, "{}", n),
            Value::Number(n) => write!(f, "{}", n),
            Value::BigInt(s) => write!(f, "{}n", s),
            Value::String(s) => write!(f, "{}", s),
            Value::Symbol(_) => write!(f, "Symbol()"),
            Value::Object(_) => write!(f, "[object Object]"),
            Value::Array(_) => write!(f, "[object Array]"),
            Value::Map(_) => write!(f, "[object Map]"),
            Value::Set(_) => write!(f, "[object Set]"),
            Value::Date(_) => write!(f, "[object Date]"),
            Value::Function(_)
            | Value::DynamicFunction(_)
            | Value::Builtin(_)
            | Value::BoundBuiltin(_, _, _)
            | Value::BoundFunction(_, _, _) => {
                write!(f, "[object Function]")
            }
            Value::Generator(_) => write!(f, "[object Generator]"),
            Value::Promise(_) => write!(f, "[object Promise]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_equality() {
        assert_eq!(Value::Undefined, Value::Undefined);
        assert_eq!(Value::Int(42), Value::Int(42));
        assert_ne!(Value::Int(1), Value::Int(2));
    }
}
