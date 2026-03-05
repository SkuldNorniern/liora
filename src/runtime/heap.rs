use super::Value;
use crate::ir::bytecode::BytecodeChunk;
use crate::runtime::builtins;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum GeneratorStatus {
    NotStarted,
    Suspended,
    Completed,
}

#[derive(Debug, Clone)]
pub enum PromiseState {
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Debug, Clone)]
pub struct PromiseRecord {
    pub state: PromiseState,
    /// Callbacks registered via .then(onFulfilled, onRejected)
    pub callbacks: Vec<(Value, Value)>,
}

#[derive(Debug, Clone)]
pub struct GeneratorState {
    pub chunk: BytecodeChunk,
    pub is_dynamic: bool,
    pub dyn_index: usize,
    pub pc: usize,
    pub locals: Vec<Value>,
    pub operand_stack: Vec<Value>,
    pub status: GeneratorStatus,
    pub this_value: Value,
}

fn b(category: &str, name: &str) -> u8 {
    builtins::resolve(category, name).unwrap_or_else(|| panic!("builtin {}::{}", category, name))
}

const MAX_ARRAY_LENGTH: usize = 10_000_000;

#[derive(Debug)]
struct HeapObject {
    props: HashMap<String, Value>,
    prototype: Option<usize>,
}

#[derive(Debug)]
pub struct Heap {
    objects: Vec<HeapObject>,
    arrays: Vec<Vec<Value>>,
    array_props: Vec<HashMap<String, Value>>,
    maps: Vec<std::collections::HashMap<String, Value>>,
    sets: Vec<std::collections::HashSet<String>>,
    dates: Vec<f64>,
    symbols: Vec<Option<String>>,
    error_object_ids: HashSet<usize>,
    global_object_id: usize,
    array_prototype_id: Option<usize>,
    regexp_prototype_id: Option<usize>,
    function_props: HashMap<usize, HashMap<String, Value>>,
    deleted_builtin_props: HashSet<(u8, String)>,
    is_html_dda_object_id: Option<usize>,
    /// Bytecode chunks for dynamic (closure) functions. Indexed by DynamicFunction id.
    /// Lives on the heap so DynamicFunction values remain valid across interpreter invocations.
    pub dynamic_chunks: Vec<BytecodeChunk>,
    /// Captured slot values for each dynamic function.
    pub dynamic_captures: Vec<Vec<(u32, Value)>>,
    /// Named properties for dynamic functions (e.g. `prototype`). Indexed by dynamic function id.
    dynamic_function_props: Vec<HashMap<String, Value>>,
    /// Suspended generator states. Indexed by the usize in Value::Generator.
    pub generator_states: Vec<GeneratorState>,
    /// Promise records. Indexed by the usize in Value::Promise.
    pub promises: Vec<PromiseRecord>,
}

impl Default for Heap {
    fn default() -> Self {
        let mut heap = Self {
            objects: Vec::new(),
            arrays: Vec::new(),
            array_props: Vec::new(),
            maps: Vec::new(),
            sets: Vec::new(),
            dates: Vec::new(),
            symbols: Vec::new(),
            error_object_ids: HashSet::new(),
            global_object_id: 0,
            array_prototype_id: None,
            regexp_prototype_id: None,
            function_props: HashMap::new(),
            deleted_builtin_props: HashSet::new(),
            is_html_dda_object_id: None,
            dynamic_chunks: Vec::new(),
            dynamic_captures: Vec::new(),
            dynamic_function_props: Vec::new(),
            generator_states: Vec::new(),
            promises: Vec::new(),
        };
        heap.init_globals();
        heap
    }
}

impl Heap {
    pub fn new() -> Self {
        Self::default()
    }

    fn init_globals(&mut self) {
        let global_id = self.alloc_object();
        self.global_object_id = global_id;

        let obj_proto_id = self.alloc_object();
        self.set_prop(
            obj_proto_id,
            "toString",
            Value::Builtin(b("Object", "toString")),
        );
        self.set_prop(
            obj_proto_id,
            "hasOwnProperty",
            Value::Builtin(b("Object", "hasOwnProperty")),
        );
        self.set_prop(
            obj_proto_id,
            "propertyIsEnumerable",
            Value::Builtin(b("Object", "propertyIsEnumerable")),
        );
        let obj_id = self.alloc_object();
        self.set_prop(obj_id, "prototype", Value::Object(obj_proto_id));
        self.set_prop(obj_id, "create", Value::Builtin(b("Object", "create")));
        self.set_prop(obj_id, "keys", Value::Builtin(b("Object", "keys")));
        self.set_prop(obj_id, "values", Value::Builtin(b("Object", "values")));
        self.set_prop(obj_id, "entries", Value::Builtin(b("Object", "entries")));
        self.set_prop(obj_id, "assign", Value::Builtin(b("Object", "assign")));
        self.set_prop(
            obj_id,
            "hasOwnProperty",
            Value::Builtin(b("Object", "hasOwnProperty")),
        );
        self.set_prop(
            obj_id,
            "preventExtensions",
            Value::Builtin(b("Object", "preventExtensions")),
        );
        self.set_prop(obj_id, "seal", Value::Builtin(b("Object", "seal")));
        self.set_prop(
            obj_id,
            "setPrototypeOf",
            Value::Builtin(b("Object", "setPrototypeOf")),
        );
        self.set_prop(
            obj_id,
            "propertyIsEnumerable",
            Value::Builtin(b("Object", "propertyIsEnumerable")),
        );
        self.set_prop(
            obj_id,
            "getPrototypeOf",
            Value::Builtin(b("Object", "getPrototypeOf")),
        );
        self.set_prop(obj_id, "freeze", Value::Builtin(b("Object", "freeze")));
        self.set_prop(
            obj_id,
            "isExtensible",
            Value::Builtin(b("Object", "isExtensible")),
        );
        self.set_prop(obj_id, "isFrozen", Value::Builtin(b("Object", "isFrozen")));
        self.set_prop(obj_id, "isSealed", Value::Builtin(b("Object", "isSealed")));
        self.set_prop(obj_id, "hasOwn", Value::Builtin(b("Object", "hasOwn")));
        self.set_prop(obj_id, "is", Value::Builtin(b("Object", "is")));
        self.set_prop(
            obj_id,
            "fromEntries",
            Value::Builtin(b("Object", "fromEntries")),
        );
        self.set_prop(
            obj_id,
            "getOwnPropertyDescriptor",
            Value::Builtin(b("Object", "getOwnPropertyDescriptor")),
        );
        self.set_prop(
            obj_id,
            "getOwnPropertyNames",
            Value::Builtin(b("Object", "getOwnPropertyNames")),
        );
        self.set_prop(
            obj_id,
            "defineProperty",
            Value::Builtin(b("Object", "defineProperty")),
        );
        self.set_prop(
            obj_id,
            "defineProperties",
            Value::Builtin(b("Object", "defineProperties")),
        );
        self.set_prop(global_id, "Object", Value::Object(obj_id));

        let arr_proto_id = self.alloc_object();
        self.set_prop(arr_proto_id, "push", Value::Builtin(b("Array", "push")));
        self.set_prop(arr_proto_id, "pop", Value::Builtin(b("Array", "pop")));
        self.set_prop(
            arr_proto_id,
            "isArray",
            Value::Builtin(b("Array", "isArray")),
        );
        self.set_prop(arr_proto_id, "slice", Value::Builtin(b("Array", "slice")));
        self.set_prop(arr_proto_id, "concat", Value::Builtin(b("Array", "concat")));
        self.set_prop(
            arr_proto_id,
            "indexOf",
            Value::Builtin(b("Array", "indexOf")),
        );
        self.set_prop(arr_proto_id, "join", Value::Builtin(b("Array", "join")));
        self.set_prop(arr_proto_id, "shift", Value::Builtin(b("Array", "shift")));
        self.set_prop(
            arr_proto_id,
            "unshift",
            Value::Builtin(b("Array", "unshift")),
        );
        self.set_prop(
            arr_proto_id,
            "reverse",
            Value::Builtin(b("Array", "reverse")),
        );
        self.set_prop(
            arr_proto_id,
            "includes",
            Value::Builtin(b("Array", "includes")),
        );
        self.set_prop(arr_proto_id, "at", Value::Builtin(b("Array", "at")));
        self.set_prop(arr_proto_id, "fill", Value::Builtin(b("Array", "fill")));
        self.set_prop(
            arr_proto_id,
            "lastIndexOf",
            Value::Builtin(b("Array", "lastIndexOf")),
        );
        self.set_prop(
            arr_proto_id,
            "toString",
            Value::Builtin(b("Array", "toString")),
        );
        self.set_prop(arr_proto_id, "length", Value::Int(0));
        self.set_prop(arr_proto_id, "map", Value::Builtin(b("Array", "map")));
        self.set_prop(arr_proto_id, "reduce", Value::Builtin(b("Array", "reduce")));
        self.set_prop(
            arr_proto_id,
            "reduceRight",
            Value::Builtin(b("Array", "reduceRight")),
        );
        self.set_prop(arr_proto_id, "some", Value::Builtin(b("Array", "some")));
        self.set_prop(arr_proto_id, "every", Value::Builtin(b("Array", "every")));
        self.set_prop(
            arr_proto_id,
            "forEach",
            Value::Builtin(b("Array", "forEach")),
        );
        self.set_prop(arr_proto_id, "filter", Value::Builtin(b("Array", "filter")));
        self.set_prop(arr_proto_id, "splice", Value::Builtin(b("Array", "splice")));
        self.set_prop(arr_proto_id, "sort", Value::Builtin(b("Array", "sort")));
        self.set_prop(
            arr_proto_id,
            "toLocaleString",
            Value::Builtin(b("Array", "toLocaleString")),
        );
        self.set_prop(arr_proto_id, "values", Value::Builtin(b("Array", "values")));
        self.set_prop(arr_proto_id, "keys", Value::Builtin(b("Array", "keys")));
        self.set_prop(
            arr_proto_id,
            "entries",
            Value::Builtin(b("Array", "entries")),
        );
        self.set_prop(arr_proto_id, "find", Value::Builtin(b("Array", "find")));
        self.set_prop(
            arr_proto_id,
            "findIndex",
            Value::Builtin(b("Array", "findIndex")),
        );
        self.set_prop(
            arr_proto_id,
            "findLast",
            Value::Builtin(b("Array", "findLast")),
        );
        self.set_prop(
            arr_proto_id,
            "findLastIndex",
            Value::Builtin(b("Array", "findLastIndex")),
        );
        self.set_prop(arr_proto_id, "flat", Value::Builtin(b("Array", "flat")));
        self.set_prop(
            arr_proto_id,
            "flatMap",
            Value::Builtin(b("Array", "flatMap")),
        );
        self.set_prop(
            arr_proto_id,
            "copyWithin",
            Value::Builtin(b("Array", "copyWithin")),
        );
        self.set_prop(
            arr_proto_id,
            "toReversed",
            Value::Builtin(b("Array", "toReversed")),
        );
        self.set_prop(
            arr_proto_id,
            "toSorted",
            Value::Builtin(b("Array", "toSorted")),
        );
        self.set_prop(
            arr_proto_id,
            "toSpliced",
            Value::Builtin(b("Array", "toSpliced")),
        );
        self.set_prop(arr_proto_id, "with", Value::Builtin(b("Array", "with")));
        self.array_prototype_id = Some(arr_proto_id);

        let arr_id = self.alloc_object();
        self.set_prop(arr_id, "prototype", Value::Object(arr_proto_id));
        self.set_prop(arr_id, "isArray", Value::Builtin(b("Array", "isArray")));
        self.set_prop(arr_id, "from", Value::Builtin(b("Array", "from")));
        self.set_prop(arr_id, "of", Value::Builtin(b("Array", "of")));
        self.set_prop(arr_id, "__call__", Value::Builtin(b("Array", "create")));
        self.set_prop(global_id, "Array", Value::Object(arr_id));

        let math_id = self.alloc_object();
        self.set_prop(math_id, "floor", Value::Builtin(b("Math", "floor")));
        self.set_prop(math_id, "abs", Value::Builtin(b("Math", "abs")));
        self.set_prop(math_id, "min", Value::Builtin(b("Math", "min")));
        self.set_prop(math_id, "max", Value::Builtin(b("Math", "max")));
        self.set_prop(math_id, "pow", Value::Builtin(b("Math", "pow")));
        self.set_prop(math_id, "ceil", Value::Builtin(b("Math", "ceil")));
        self.set_prop(math_id, "round", Value::Builtin(b("Math", "round")));
        self.set_prop(math_id, "sqrt", Value::Builtin(b("Math", "sqrt")));
        self.set_prop(math_id, "random", Value::Builtin(b("Math", "random")));
        self.set_prop(math_id, "sign", Value::Builtin(b("Math", "sign")));
        self.set_prop(math_id, "trunc", Value::Builtin(b("Math", "trunc")));
        self.set_prop(
            math_id,
            "sumPrecise",
            Value::Builtin(b("Math", "sumPrecise")),
        );
        self.set_prop(global_id, "Math", Value::Object(math_id));

        let json_id = self.alloc_object();
        self.set_prop(json_id, "parse", Value::Builtin(b("Json", "parse")));
        self.set_prop(json_id, "stringify", Value::Builtin(b("Json", "stringify")));
        self.set_prop(global_id, "JSON", Value::Object(json_id));

        let str_proto_id = self.alloc_object();
        self.set_prop(str_proto_id, "split", Value::Builtin(b("String", "split")));
        self.set_prop(str_proto_id, "match", Value::Builtin(b("String", "match")));
        self.set_prop(
            str_proto_id,
            "search",
            Value::Builtin(b("String", "search")),
        );
        self.set_prop(
            str_proto_id,
            "replace",
            Value::Builtin(b("String", "replace")),
        );
        self.set_prop(
            str_proto_id,
            "replaceAll",
            Value::Builtin(b("String", "replace")),
        );
        self.set_prop(str_proto_id, "trim", Value::Builtin(b("String", "trim")));
        self.set_prop(
            str_proto_id,
            "startsWith",
            Value::Builtin(b("String", "startsWith")),
        );
        self.set_prop(
            str_proto_id,
            "endsWith",
            Value::Builtin(b("String", "endsWith")),
        );
        self.set_prop(
            str_proto_id,
            "toLowerCase",
            Value::Builtin(b("String", "toLowerCase")),
        );
        self.set_prop(
            str_proto_id,
            "toUpperCase",
            Value::Builtin(b("String", "toUpperCase")),
        );
        self.set_prop(
            str_proto_id,
            "charAt",
            Value::Builtin(b("String", "charAt")),
        );
        self.set_prop(str_proto_id, "at", Value::Builtin(b("String", "at")));
        self.set_prop(
            str_proto_id,
            "includes",
            Value::Builtin(b("String", "includes")),
        );
        self.set_prop(
            str_proto_id,
            "padStart",
            Value::Builtin(b("String", "padStart")),
        );
        self.set_prop(
            str_proto_id,
            "padEnd",
            Value::Builtin(b("String", "padEnd")),
        );
        self.set_prop(
            str_proto_id,
            "indexOf",
            Value::Builtin(b("Array", "indexOf")),
        );
        self.set_prop(
            str_proto_id,
            "lastIndexOf",
            Value::Builtin(b("Array", "lastIndexOf")),
        );
        self.set_prop(
            str_proto_id,
            "repeat",
            Value::Builtin(b("String", "repeat")),
        );
        self.set_prop(
            str_proto_id,
            "anchor",
            Value::Builtin(b("String", "anchor")),
        );
        self.set_prop(str_proto_id, "big", Value::Builtin(b("String", "big")));
        self.set_prop(str_proto_id, "blink", Value::Builtin(b("String", "blink")));
        self.set_prop(str_proto_id, "bold", Value::Builtin(b("String", "bold")));
        self.set_prop(str_proto_id, "fixed", Value::Builtin(b("String", "fixed")));
        self.set_prop(
            str_proto_id,
            "fontcolor",
            Value::Builtin(b("String", "fontcolor")),
        );
        self.set_prop(
            str_proto_id,
            "fontsize",
            Value::Builtin(b("String", "fontsize")),
        );
        self.set_prop(
            str_proto_id,
            "italics",
            Value::Builtin(b("String", "italics")),
        );
        self.set_prop(str_proto_id, "link", Value::Builtin(b("String", "link")));
        self.set_prop(str_proto_id, "small", Value::Builtin(b("String", "small")));
        self.set_prop(
            str_proto_id,
            "strike",
            Value::Builtin(b("String", "strike")),
        );
        self.set_prop(str_proto_id, "sub", Value::Builtin(b("String", "sub")));
        self.set_prop(
            str_proto_id,
            "substr",
            Value::Builtin(b("String", "substr")),
        );
        self.set_prop(str_proto_id, "sup", Value::Builtin(b("String", "sup")));
        self.set_prop(
            str_proto_id,
            "trimLeft",
            Value::Builtin(b("String", "trimLeft")),
        );
        self.set_prop(
            str_proto_id,
            "trimRight",
            Value::Builtin(b("String", "trimRight")),
        );
        let str_id = self.alloc_object();
        self.set_prop(str_id, "prototype", Value::Object(str_proto_id));
        self.set_prop(
            str_id,
            "fromCharCode",
            Value::Builtin(b("String", "fromCharCode")),
        );
        self.set_prop(global_id, "String", Value::Object(str_id));

        let num_id = self.alloc_object();
        self.set_prop(num_id, "__call__", Value::Builtin(b("Type", "Number")));
        self.set_prop(num_id, "EPSILON", Value::Number(2.0_f64.powi(-52)));
        self.set_prop(
            num_id,
            "MIN_SAFE_INTEGER",
            Value::Number(-9007199254740991.0),
        );
        self.set_prop(
            num_id,
            "MAX_SAFE_INTEGER",
            Value::Number(9007199254740991.0),
        );
        self.set_prop(
            num_id,
            "isInteger",
            Value::Builtin(b("Number", "isInteger")),
        );
        self.set_prop(
            num_id,
            "isSafeInteger",
            Value::Builtin(b("Number", "isSafeInteger")),
        );
        self.set_prop(num_id, "isFinite", Value::Builtin(b("Number", "isFinite")));
        self.set_prop(num_id, "isNaN", Value::Builtin(b("Number", "isNaN")));
        self.set_prop(global_id, "Number", Value::Object(num_id));

        self.set_prop(global_id, "Boolean", Value::Builtin(b("Type", "Boolean")));

        let err_id = self.alloc_object();
        self.set_prop(err_id, "isError", Value::Builtin(b("Error", "isError")));
        self.set_prop(err_id, "__call__", Value::Builtin(b("Type", "Error")));
        self.set_prop(global_id, "Error", Value::Object(err_id));

        let ref_err_id = self.alloc_object();
        self.set_prop(
            ref_err_id,
            "name",
            Value::String("ReferenceError".to_string()),
        );
        self.set_prop(ref_err_id, "__call__", Value::Builtin(b("Type", "Error")));
        self.set_prop(global_id, "ReferenceError", Value::Object(ref_err_id));

        let type_err_id = self.alloc_object();
        self.set_prop(type_err_id, "name", Value::String("TypeError".to_string()));
        self.set_prop(type_err_id, "__call__", Value::Builtin(b("Type", "Error")));
        self.set_prop(global_id, "TypeError", Value::Object(type_err_id));

        let range_err_id = self.alloc_object();
        self.set_prop(
            range_err_id,
            "name",
            Value::String("RangeError".to_string()),
        );
        self.set_prop(range_err_id, "__call__", Value::Builtin(b("Type", "Error")));
        self.set_prop(global_id, "RangeError", Value::Object(range_err_id));

        let syntax_err_id = self.alloc_object();
        self.set_prop(
            syntax_err_id,
            "name",
            Value::String("SyntaxError".to_string()),
        );
        self.set_prop(
            syntax_err_id,
            "__call__",
            Value::Builtin(b("Type", "Error")),
        );
        self.set_prop(global_id, "SyntaxError", Value::Object(syntax_err_id));

        let uri_err_id = self.alloc_object();
        self.set_prop(uri_err_id, "name", Value::String("URIError".to_string()));
        self.set_prop(uri_err_id, "__call__", Value::Builtin(b("Type", "Error")));
        self.set_prop(global_id, "URIError", Value::Object(uri_err_id));

        let eval_err_id = self.alloc_object();
        self.set_prop(eval_err_id, "name", Value::String("EvalError".to_string()));
        self.set_prop(eval_err_id, "__call__", Value::Builtin(b("Type", "Error")));
        self.set_prop(global_id, "EvalError", Value::Object(eval_err_id));

        let aggregate_err_id = self.alloc_object();
        self.set_prop(
            aggregate_err_id,
            "name",
            Value::String("AggregateError".to_string()),
        );
        self.set_prop(
            aggregate_err_id,
            "__call__",
            Value::Builtin(b("Type", "Error")),
        );
        self.set_prop(global_id, "AggregateError", Value::Object(aggregate_err_id));

        let regexp_proto_id = self.alloc_object();
        self.regexp_prototype_id = Some(regexp_proto_id);
        self.set_prop(regexp_proto_id, "exec", Value::Builtin(b("RegExp", "exec")));
        self.set_prop(regexp_proto_id, "test", Value::Builtin(b("RegExp", "test")));
        self.set_prop(
            regexp_proto_id,
            "compile",
            Value::Builtin(b("RegExp", "compile")),
        );
        self.set_prop(
            regexp_proto_id,
            "Symbol.match",
            Value::Builtin(b("RegExp", "symbol_match")),
        );
        self.set_prop(
            regexp_proto_id,
            "Symbol.search",
            Value::Builtin(b("RegExp", "symbol_search")),
        );
        self.set_prop(
            regexp_proto_id,
            "Symbol.replace",
            Value::Builtin(b("RegExp", "symbol_replace")),
        );
        self.set_prop(
            regexp_proto_id,
            "Symbol.split",
            Value::Builtin(b("RegExp", "symbol_split")),
        );
        let regexp_id = self.alloc_object();
        self.set_prop(regexp_id, "prototype", Value::Object(regexp_proto_id));
        self.set_prop(regexp_id, "escape", Value::Builtin(b("RegExp", "escape")));
        self.set_prop(regexp_id, "__call__", Value::Builtin(b("RegExp", "create")));
        self.set_prop(global_id, "RegExp", Value::Object(regexp_id));

        let map_id = self.alloc_object();
        self.set_prop(map_id, "set", Value::Builtin(b("Map", "set")));
        self.set_prop(map_id, "get", Value::Builtin(b("Map", "get")));
        self.set_prop(map_id, "has", Value::Builtin(b("Map", "has")));
        self.set_prop(global_id, "Map", Value::Object(map_id));

        let set_id = self.alloc_object();
        self.set_prop(set_id, "add", Value::Builtin(b("Set", "add")));
        self.set_prop(set_id, "has", Value::Builtin(b("Set", "has")));
        self.set_prop(set_id, "size", Value::Builtin(b("Set", "size")));
        self.set_prop(global_id, "Set", Value::Object(set_id));

        let date_id = self.alloc_object();
        let date_proto_id = self.alloc_object();
        self.set_prop(
            date_proto_id,
            "getTime",
            Value::Builtin(b("Date", "getTime")),
        );
        self.set_prop(
            date_proto_id,
            "toString",
            Value::Builtin(b("Date", "toString")),
        );
        self.set_prop(
            date_proto_id,
            "toISOString",
            Value::Builtin(b("Date", "toISOString")),
        );
        self.set_prop(
            date_proto_id,
            "getYear",
            Value::Builtin(b("Date", "getYear")),
        );
        self.set_prop(
            date_proto_id,
            "getFullYear",
            Value::Builtin(b("Date", "getFullYear")),
        );
        self.set_prop(
            date_proto_id,
            "setYear",
            Value::Builtin(b("Date", "setYear")),
        );
        self.set_prop(
            date_proto_id,
            "toGMTString",
            Value::Builtin(b("Date", "toGMTString")),
        );
        self.set_prop(date_id, "prototype", Value::Object(date_proto_id));
        self.set_prop(date_id, "__call__", Value::Builtin(b("Date", "create")));
        self.set_prop(date_id, "now", Value::Builtin(b("Date", "now")));
        self.set_prop(date_id, "getTime", Value::Builtin(b("Date", "getTime")));
        self.set_prop(date_id, "toString", Value::Builtin(b("Date", "toString")));
        self.set_prop(
            date_id,
            "toISOString",
            Value::Builtin(b("Date", "toISOString")),
        );
        self.set_prop(global_id, "Date", Value::Object(date_id));

        self.set_prop(global_id, "NaN", Value::Number(f64::NAN));
        self.set_prop(global_id, "Infinity", Value::Number(f64::INFINITY));
        self.set_prop(global_id, "globalThis", Value::Object(global_id));

        let sym_match = self.alloc_symbol(Some("Symbol.match".to_string()));
        let sym_replace = self.alloc_symbol(Some("Symbol.replace".to_string()));
        let sym_search = self.alloc_symbol(Some("Symbol.search".to_string()));
        let sym_split = self.alloc_symbol(Some("Symbol.split".to_string()));
        let sym_iterator = self.alloc_symbol(Some("Symbol.iterator".to_string()));
        let sym_species = self.alloc_symbol(Some("Symbol.species".to_string()));
        let sym_to_string_tag = self.alloc_symbol(Some("Symbol.toStringTag".to_string()));
        let symbol_id = self.alloc_object();
        self.set_prop(symbol_id, "__call__", Value::Builtin(b("Symbol", "create")));
        self.set_prop(symbol_id, "match", Value::Symbol(sym_match));
        self.set_prop(symbol_id, "replace", Value::Symbol(sym_replace));
        self.set_prop(symbol_id, "search", Value::Symbol(sym_search));
        self.set_prop(symbol_id, "split", Value::Symbol(sym_split));
        self.set_prop(symbol_id, "iterator", Value::Symbol(sym_iterator));
        self.set_prop(symbol_id, "species", Value::Symbol(sym_species));
        self.set_prop(symbol_id, "toStringTag", Value::Symbol(sym_to_string_tag));
        self.set_prop(global_id, "Symbol", Value::Object(symbol_id));

        let console_id = self.alloc_object();
        self.set_prop(console_id, "log", Value::Builtin(b("Host", "print")));
        self.set_prop(global_id, "console", Value::Object(console_id));

        self.set_prop(global_id, "print", Value::Builtin(b("Host", "print")));
        self.set_prop(global_id, "eval", Value::Builtin(b("Global", "eval")));
        self.set_prop(
            global_id,
            "encodeURI",
            Value::Builtin(b("Global", "encodeURI")),
        );
        self.set_prop(
            global_id,
            "encodeURIComponent",
            Value::Builtin(b("Global", "encodeURIComponent")),
        );
        self.set_prop(
            global_id,
            "decodeURI",
            Value::Builtin(b("Global", "decodeURI")),
        );
        self.set_prop(
            global_id,
            "decodeURIComponent",
            Value::Builtin(b("Global", "decodeURIComponent")),
        );
        self.set_prop(
            global_id,
            "parseInt",
            Value::Builtin(b("Global", "parseInt")),
        );
        self.set_prop(
            global_id,
            "parseFloat",
            Value::Builtin(b("Global", "parseFloat")),
        );
        self.set_prop(global_id, "escape", Value::Builtin(b("Global", "escape")));
        self.set_prop(
            global_id,
            "unescape",
            Value::Builtin(b("Global", "unescape")),
        );
        let int32array = b("TypedArray", "Int32Array");
        self.set_prop(global_id, "Int32Array", Value::Builtin(int32array));
        self.set_prop(global_id, "Int8Array", Value::Builtin(int32array));
        self.set_prop(global_id, "Int16Array", Value::Builtin(int32array));
        self.set_prop(
            global_id,
            "Uint8Array",
            Value::Builtin(b("TypedArray", "Uint8Array")),
        );
        self.set_prop(
            global_id,
            "Uint8ClampedArray",
            Value::Builtin(b("TypedArray", "Uint8ClampedArray")),
        );
        self.set_prop(global_id, "Uint16Array", Value::Builtin(int32array));
        self.set_prop(global_id, "Uint32Array", Value::Builtin(int32array));
        self.set_prop(global_id, "Float32Array", Value::Builtin(int32array));
        self.set_prop(global_id, "Float64Array", Value::Builtin(int32array));
        self.set_prop(global_id, "Float16Array", Value::Builtin(int32array));
        self.set_prop(global_id, "BigInt64Array", Value::Builtin(int32array));
        self.set_prop(global_id, "BigUint64Array", Value::Builtin(int32array));
        self.set_prop(
            global_id,
            "ArrayBuffer",
            Value::Builtin(b("TypedArray", "ArrayBuffer")),
        );
        let func_proto_id = self.alloc_object();
        self.set_prop(func_proto_id, "call", Value::Builtin(b("Function", "call")));
        self.set_prop(func_proto_id, "bind", Value::Builtin(b("Function", "bind")));
        self.set_prop(
            func_proto_id,
            "apply",
            Value::Builtin(b("Function", "apply")),
        );
        let func_id = self.alloc_object();
        self.set_prop(func_id, "prototype", Value::Object(func_proto_id));
        self.set_prop(func_id, "__call__", Value::Builtin(b("Global", "Function")));
        self.set_prop(global_id, "Function", Value::Object(func_id));
        let promise_id = self.alloc_object();
        let promise_proto_id = self.alloc_object();
        self.set_prop(promise_id, "prototype", Value::Object(promise_proto_id));
        self.set_prop(
            promise_id,
            "resolve",
            Value::Builtin(b("Promise", "resolve_static")),
        );
        self.set_prop(
            promise_id,
            "reject",
            Value::Builtin(b("Promise", "reject_static")),
        );
        self.set_prop(promise_id, "all", Value::Builtin(b("Promise", "all")));
        self.set_prop(global_id, "Promise", Value::Object(promise_id));
        self.set_prop(global_id, "isNaN", Value::Builtin(b("Global", "isNaN")));
        self.set_prop(
            global_id,
            "isFinite",
            Value::Builtin(b("Global", "isFinite")),
        );
        let reflect_id = self.alloc_object();
        self.set_prop(reflect_id, "get", Value::Builtin(b("Reflect", "get")));
        self.set_prop(reflect_id, "apply", Value::Builtin(b("Reflect", "apply")));
        self.set_prop(
            reflect_id,
            "construct",
            Value::Builtin(b("Reflect", "construct")),
        );
        self.set_prop(global_id, "Reflect", Value::Object(reflect_id));
        let weakmap_id = self.alloc_object();
        self.set_prop(global_id, "WeakMap", Value::Object(weakmap_id));
        let weakset_id = self.alloc_object();
        self.set_prop(global_id, "WeakSet", Value::Object(weakset_id));
        let proxy_id = self.alloc_object();
        self.set_prop(global_id, "Proxy", Value::Object(proxy_id));
        self.set_prop(
            global_id,
            "DataView",
            Value::Builtin(b("TypedArray", "DataView")),
        );
    }

    /// Node-compat globals (require, process). Opt-in via --compat. Stubs only.
    pub fn init_compat_globals(&mut self) {
        let global_id = self.global_object_id;
        let require_builtin = builtins::resolve("Compat", "require").expect("Compat.require");
        self.set_prop(global_id, "require", Value::Builtin(require_builtin));
        let process_id = self.alloc_object();
        let env_id = self.alloc_object();
        self.set_prop(process_id, "env", Value::Object(env_id));
        self.set_prop(global_id, "process", Value::Object(process_id));
    }

    /// Add $262 host object for test262 harness. Match V8/Bun/Deno: $262 only exists when running via test262.
    pub fn init_test262_globals(&mut self) {
        let global_id = self.global_object_id;
        let dollar262_id = self.alloc_object();
        let is_html_dda_id = self.alloc_object();
        self.is_html_dda_object_id = Some(is_html_dda_id);
        self.set_prop(dollar262_id, "IsHTMLDDA", Value::Object(is_html_dda_id));
        self.set_prop(dollar262_id, "global", Value::Object(global_id));
        self.set_prop(
            dollar262_id,
            "createRealm",
            Value::Builtin(b("$262", "createRealm")),
        );
        self.set_prop(
            dollar262_id,
            "evalScript",
            Value::Builtin(b("$262", "evalScript")),
        );
        self.set_prop(dollar262_id, "gc", Value::Builtin(b("$262", "gc")));
        self.set_prop(
            dollar262_id,
            "detachArrayBuffer",
            Value::Builtin(b("$262", "detachArrayBuffer")),
        );
        self.set_prop(global_id, "$262", Value::Object(dollar262_id));
        self.set_prop(global_id, "global", Value::Object(global_id));
        self.set_prop(global_id, "timeout", Value::Builtin(b("Host", "timeout")));
        let temporal_id = self.alloc_object();
        self.set_prop(global_id, "Temporal", Value::Object(temporal_id));
        let intl_id = self.alloc_object();
        self.set_prop(global_id, "Intl", Value::Object(intl_id));
        self.set_prop(global_id, "testResult", Value::Undefined);
    }

    pub fn is_html_dda_object(&self, obj_id: usize) -> bool {
        self.is_html_dda_object_id == Some(obj_id)
    }

    pub fn get_global(&self, name: &str) -> Value {
        self.get_prop(self.global_object_id, name)
    }

    pub fn get_function_prop(&self, func_index: usize, key: &str) -> Value {
        self.function_props
            .get(&func_index)
            .and_then(|m| m.get(key).cloned())
            .unwrap_or(Value::Undefined)
    }

    pub fn set_function_prop(&mut self, func_index: usize, key: &str, value: Value) {
        let props = self.function_props.entry(func_index).or_default();
        if key == "name" && props.contains_key("name") {
            return;
        }
        props.insert(key.to_string(), value);
    }

    pub fn get_dynamic_function_prop(&self, dyn_idx: usize, key: &str) -> Value {
        self.dynamic_function_props
            .get(dyn_idx)
            .and_then(|m| m.get(key).cloned())
            .unwrap_or(Value::Undefined)
    }

    pub fn set_dynamic_function_prop(&mut self, dyn_idx: usize, key: &str, value: Value) {
        if dyn_idx >= self.dynamic_function_props.len() {
            self.dynamic_function_props
                .resize_with(dyn_idx + 1, HashMap::new);
        }
        self.dynamic_function_props[dyn_idx].insert(key.to_string(), value);
    }

    pub fn alloc_generator(&mut self, state: GeneratorState) -> usize {
        let id = self.generator_states.len();
        self.generator_states.push(state);
        id
    }

    pub fn get_generator(&self, id: usize) -> Option<&GeneratorState> {
        self.generator_states.get(id)
    }

    pub fn get_generator_mut(&mut self, id: usize) -> Option<&mut GeneratorState> {
        self.generator_states.get_mut(id)
    }

    pub fn alloc_promise(&mut self, state: PromiseState) -> usize {
        let id = self.promises.len();
        self.promises.push(PromiseRecord {
            state,
            callbacks: Vec::new(),
        });
        id
    }

    pub fn get_promise(&self, id: usize) -> Option<&PromiseRecord> {
        self.promises.get(id)
    }

    pub fn get_promise_mut(&mut self, id: usize) -> Option<&mut PromiseRecord> {
        self.promises.get_mut(id)
    }

    pub fn function_has_own_property(&self, func_index: usize, key: &str) -> bool {
        self.function_props
            .get(&func_index)
            .map(|props| props.contains_key(key))
            .unwrap_or(false)
    }

    pub fn function_keys(&self, func_index: usize) -> Vec<String> {
        self.function_props
            .get(&func_index)
            .map(|props| props.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn delete_function_prop(&mut self, func_index: usize, key: &str) {
        if let Some(props) = self.function_props.get_mut(&func_index) {
            props.remove(key);
        }
    }

    pub fn global_object(&self) -> usize {
        self.global_object_id
    }

    pub fn alloc_object(&mut self) -> usize {
        self.alloc_object_with_prototype(None)
    }

    pub fn alloc_regexp(&mut self) -> usize {
        self.alloc_object_with_prototype(self.regexp_prototype_id)
    }

    pub fn alloc_object_with_prototype(&mut self, prototype: Option<usize>) -> usize {
        let id = self.objects.len();
        self.objects.push(HeapObject {
            props: HashMap::new(),
            prototype,
        });
        id
    }

    pub fn get_proto(&self, obj_id: usize) -> Option<usize> {
        self.objects.get(obj_id).and_then(|o| o.prototype)
    }

    pub fn alloc_array(&mut self) -> usize {
        let id = self.arrays.len();
        self.arrays.push(Vec::new());
        self.array_props.push(HashMap::new());
        id
    }

    pub fn alloc_symbol(&mut self, description: Option<String>) -> usize {
        let id = self.symbols.len();
        self.symbols.push(description);
        id
    }

    pub fn symbol_description(&self, id: usize) -> Option<&str> {
        self.symbols.get(id).and_then(|o| o.as_deref())
    }

    pub fn alloc_map(&mut self) -> usize {
        let id = self.maps.len();
        self.maps.push(std::collections::HashMap::new());
        id
    }

    pub fn map_set(&mut self, map_id: usize, key: &str, value: Value) {
        if let Some(m) = self.maps.get_mut(map_id) {
            m.insert(key.to_string(), value);
        }
    }

    pub fn map_get(&self, map_id: usize, key: &str) -> Value {
        self.maps
            .get(map_id)
            .and_then(|m| m.get(key).cloned())
            .unwrap_or(Value::Undefined)
    }

    pub fn map_has(&self, map_id: usize, key: &str) -> bool {
        self.maps
            .get(map_id)
            .map(|m| m.contains_key(key))
            .unwrap_or(false)
    }

    pub fn map_size(&self, map_id: usize) -> usize {
        self.maps.get(map_id).map(|m| m.len()).unwrap_or(0)
    }

    pub fn alloc_set(&mut self) -> usize {
        let id = self.sets.len();
        self.sets.push(std::collections::HashSet::new());
        id
    }

    pub fn set_add(&mut self, set_id: usize, key: &str) {
        if let Some(s) = self.sets.get_mut(set_id) {
            s.insert(key.to_string());
        }
    }

    pub fn set_has(&self, set_id: usize, key: &str) -> bool {
        self.sets
            .get(set_id)
            .map(|s| s.contains(key))
            .unwrap_or(false)
    }

    pub fn set_size(&self, set_id: usize) -> usize {
        self.sets.get(set_id).map(|s| s.len()).unwrap_or(0)
    }

    pub fn alloc_date(&mut self, timestamp_ms: f64) -> usize {
        let id = self.dates.len();
        self.dates.push(timestamp_ms);
        id
    }

    pub fn date_timestamp(&self, date_id: usize) -> f64 {
        self.dates.get(date_id).copied().unwrap_or(0.0)
    }

    pub fn set_date_timestamp(&mut self, date_id: usize, ms: f64) {
        if let Some(slot) = self.dates.get_mut(date_id) {
            *slot = ms;
        }
    }

    #[inline(always)]
    pub fn get_prop(&self, obj_id: usize, key: &str) -> Value {
        let mut current = Some(obj_id);
        while let Some(id) = current {
            if let Some(obj) = self.objects.get(id) {
                if let Some(v) = obj.props.get(key) {
                    return v.clone();
                }
                current = obj.prototype;
            } else {
                break;
            }
        }
        Value::Undefined
    }

    pub fn set_prototype(&mut self, obj_id: usize, prototype: Option<usize>) {
        if let Some(obj) = self.objects.get_mut(obj_id) {
            obj.prototype = prototype;
        }
    }

    #[inline(always)]
    pub fn get_array_prop(&self, arr_id: usize, key: &str) -> Value {
        if let Some(elements) = self.arrays.get(arr_id) {
            if key.as_bytes() == b"length" {
                return Value::Int(elements.len() as i32);
            }
            if let Ok(idx) = key.parse::<usize>() {
                if idx < elements.len() {
                    return elements[idx].clone();
                }
                if let Some(props) = self.array_props.get(arr_id) {
                    if let Some(v) = props.get(key) {
                        return v.clone();
                    }
                }
            }
            if let Some(props) = self.array_props.get(arr_id) {
                if let Some(v) = props.get(key) {
                    return v.clone();
                }
            }
        }
        if let Some(proto_id) = self.array_prototype_id {
            return self.get_prop(proto_id, key);
        }
        Value::Undefined
    }

    pub fn set_prop(&mut self, obj_id: usize, key: &str, value: Value) {
        if let Some(obj) = self.objects.get_mut(obj_id) {
            obj.props.insert(key.to_string(), value);
        }
    }

    pub fn delete_prop(&mut self, obj_id: usize, key: &str) {
        if let Some(obj) = self.objects.get_mut(obj_id) {
            obj.props.remove(key);
        }
    }

    pub fn delete_array_prop(&mut self, arr_id: usize, key: &str) {
        if key == "length" {
            return;
        }
        if let Ok(idx) = key.parse::<usize>() {
            if let Some(elements) = self.arrays.get_mut(arr_id) {
                if idx < elements.len() {
                    elements[idx] = Value::Undefined;
                }
            }
        } else if let Some(props) = self.array_props.get_mut(arr_id) {
            props.remove(key);
        }
    }

    pub fn set_array_prop(&mut self, arr_id: usize, key: &str, value: Value) {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            if key == "length" {
                if let Value::Int(n) = value {
                    if n >= 0 {
                        let n = n as usize;
                        elements.truncate(n.min(MAX_ARRAY_LENGTH));
                    }
                }
                return;
            }
            if let Ok(idx) = key.parse::<usize>() {
                if idx < MAX_ARRAY_LENGTH {
                    while elements.len() <= idx {
                        elements.push(Value::Undefined);
                    }
                    elements[idx] = value;
                } else if let Some(props) = self.array_props.get_mut(arr_id) {
                    props.insert(key.to_string(), value);
                }
            } else if let Some(props) = self.array_props.get_mut(arr_id) {
                props.insert(key.to_string(), value);
            }
        }
    }

    pub fn array_push(&mut self, arr_id: usize, value: Value) {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            if elements.is_empty() {
                elements.reserve(4096);
            }
            elements.push(value);
        }
    }

    pub fn array_push_values(&mut self, arr_id: usize, values: &[Value]) -> i32 {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            elements.extend(values.iter().cloned());
            elements.len() as i32
        } else {
            0
        }
    }

    pub fn array_len(&self, arr_id: usize) -> usize {
        self.arrays
            .get(arr_id)
            .map(|elements| elements.len())
            .unwrap_or(0)
    }

    pub fn array_pop(&mut self, arr_id: usize) -> Value {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            elements.pop().unwrap_or(Value::Undefined)
        } else {
            Value::Undefined
        }
    }

    pub fn array_shift(&mut self, arr_id: usize) -> Value {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            if elements.is_empty() {
                Value::Undefined
            } else {
                elements.remove(0)
            }
        } else {
            Value::Undefined
        }
    }

    pub fn array_unshift(&mut self, arr_id: usize, values: &[Value]) -> i32 {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            for v in values.iter().rev() {
                elements.insert(0, v.clone());
            }
            elements.len() as i32
        } else {
            0
        }
    }

    pub fn array_reverse(&mut self, arr_id: usize) {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            elements.reverse();
        }
    }

    pub fn array_fill(&mut self, arr_id: usize, value: Value, start: usize, end: usize) {
        if let Some(elements) = self.arrays.get_mut(arr_id) {
            let len = elements.len();
            let end = end.min(len);
            for i in start..end {
                elements[i] = value.clone();
            }
        }
    }

    pub fn array_splice(&mut self, arr_id: usize, elements: Vec<Value>) {
        if let Some(arr) = self.arrays.get_mut(arr_id) {
            *arr = elements;
        }
    }

    pub fn object_has_own_property(&self, obj_id: usize, key: &str) -> bool {
        self.objects
            .get(obj_id)
            .map(|o| o.props.contains_key(key))
            .unwrap_or(false)
    }

    pub fn object_has_property(&self, obj_id: usize, key: &str) -> bool {
        let mut current = Some(obj_id);
        while let Some(id) = current {
            if let Some(obj) = self.objects.get(id) {
                if obj.props.contains_key(key) {
                    return true;
                }
                current = obj.prototype;
            } else {
                break;
            }
        }
        false
    }

    pub fn array_has_property(&self, arr_id: usize, key: &str) -> bool {
        if let Some(elements) = self.arrays.get(arr_id) {
            if key == "length" {
                return true;
            }
            if let Ok(idx) = key.parse::<usize>() {
                if idx < elements.len() {
                    return true;
                }
            }
            if let Some(props) = self.array_props.get(arr_id) {
                if props.contains_key(key) {
                    return true;
                }
            }
        }
        if let Some(proto_id) = self.array_prototype_id {
            return self.object_has_property(proto_id, key);
        }
        false
    }

    pub fn delete_builtin_prop(&mut self, builtin_id: u8, key: &str) {
        self.deleted_builtin_props
            .insert((builtin_id, key.to_string()));
    }

    pub fn builtin_prop_deleted(&self, builtin_id: u8, key: &str) -> bool {
        self.deleted_builtin_props
            .contains(&(builtin_id, key.to_string()))
    }

    pub fn record_error_object(&mut self, obj_id: usize) {
        self.error_object_ids.insert(obj_id);
    }

    pub fn is_error_object(&self, obj_id: usize) -> bool {
        self.error_object_ids.contains(&obj_id)
    }

    pub fn format_thrown_value(&self, v: &crate::runtime::Value) -> String {
        match v {
            crate::runtime::Value::Object(id) => {
                let name = match self.get_prop(*id, "name") {
                    crate::runtime::Value::String(s) => s,
                    _ => String::new(),
                };
                let name_str = if name.is_empty() {
                    if let crate::runtime::Value::Object(ctor_id) =
                        self.get_prop(*id, "constructor")
                    {
                        if let crate::runtime::Value::String(s) = self.get_prop(ctor_id, "name") {
                            if !s.is_empty() { s } else { String::new() }
                        } else {
                            String::new()
                        }
                    } else {
                        if self.is_error_object(*id) {
                            "Error".to_string()
                        } else {
                            String::new()
                        }
                    }
                } else {
                    name.clone()
                };
                let message_val = self.get_prop(*id, "message");
                let message = match &message_val {
                    crate::runtime::Value::String(s) => {
                        if s == "undefined" {
                            String::new()
                        } else {
                            s.clone()
                        }
                    }
                    crate::runtime::Value::Undefined | crate::runtime::Value::Null => String::new(),
                    _ => message_val.to_string(),
                };
                if self.is_error_object(*id) || !name_str.is_empty() || !message.is_empty() {
                    if message.is_empty() {
                        name_str
                    } else {
                        format!("{}: {}", name_str, message)
                    }
                } else {
                    let ctor = self.get_prop(*id, "constructor");
                    let fallback = if let crate::runtime::Value::Object(ctor_id) = ctor {
                        if let crate::runtime::Value::String(s) = self.get_prop(ctor_id, "name") {
                            if !s.is_empty() {
                                format!("[object {}]", s)
                            } else {
                                "[object Object]".to_string()
                            }
                        } else {
                            "[object Object]".to_string()
                        }
                    } else {
                        "[object Object]".to_string()
                    };
                    fallback
                }
            }
            crate::runtime::Value::Undefined => "thrown undefined".to_string(),
            _ => v.to_string(),
        }
    }

    pub fn object_keys(&self, obj_id: usize) -> Vec<String> {
        self.objects
            .get(obj_id)
            .map(|o| o.props.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn array_elements(&self, arr_id: usize) -> Option<&[Value]> {
        self.arrays.get(arr_id).map(|v| v.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_set_get_prop() {
        let mut heap = Heap::new();
        let id = heap.alloc_object();
        heap.set_prop(id, "x", Value::Int(0));
        assert_eq!(heap.get_prop(id, "x").to_i64(), 0);
        heap.set_prop(id, "x", Value::Int(42));
        assert_eq!(heap.get_prop(id, "x").to_i64(), 42);
    }

    #[test]
    fn heap_prototype_chain() {
        let mut heap = Heap::new();
        let proto = heap.alloc_object();
        heap.set_prop(proto, "y", Value::Int(10));
        let obj = heap.alloc_object_with_prototype(Some(proto));
        heap.set_prop(obj, "x", Value::Int(1));
        assert_eq!(heap.get_prop(obj, "x").to_i64(), 1);
        assert_eq!(heap.get_prop(obj, "y").to_i64(), 10);
    }

    #[test]
    fn format_thrown_value_error_like_object() {
        let mut heap = Heap::new();
        let obj = heap.alloc_object();
        heap.set_prop(obj, "name", Value::String("Test262Error".to_string()));
        heap.set_prop(obj, "message", Value::String("expected true".to_string()));
        let v = Value::Object(obj);
        assert_eq!(heap.format_thrown_value(&v), "Test262Error: expected true");
    }

    #[test]
    fn format_thrown_value_plain_object_uses_constructor_name() {
        let mut heap = Heap::new();
        let ctor = heap.alloc_object();
        heap.set_prop(ctor, "name", Value::String("CustomError".to_string()));
        let obj = heap.alloc_object();
        heap.set_prop(obj, "constructor", Value::Object(ctor));
        let v = Value::Object(obj);
        assert_eq!(heap.format_thrown_value(&v), "CustomError");
    }

    #[test]
    fn format_thrown_value_plain_object_fallback() {
        let mut heap = Heap::new();
        let ctor = heap.alloc_object();
        heap.set_prop(ctor, "name", Value::String("".to_string()));
        let obj = heap.alloc_object();
        heap.set_prop(obj, "constructor", Value::Object(ctor));
        let v = Value::Object(obj);
        assert_eq!(heap.format_thrown_value(&v), "[object Object]");
    }

    #[test]
    fn delete_builtin_prop_tracks_deletion() {
        let mut heap = Heap::new();
        let id = crate::runtime::builtins::resolve("String", "anchor").expect("anchor");
        assert!(!heap.builtin_prop_deleted(id, "length"));
        heap.delete_builtin_prop(id, "length");
        assert!(heap.builtin_prop_deleted(id, "length"));
    }
}
