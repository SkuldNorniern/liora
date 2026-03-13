use super::{Heap, b};
use crate::runtime::Value;
use crate::runtime::builtins;

impl Heap {
    pub(super) fn init_globals(&mut self) {
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
        self.set_prop(obj_proto_id, "constructor", Value::Object(obj_id));
        self.set_prop(obj_id, "prototype", Value::Object(obj_proto_id));
        self.set_prop(obj_id, "__call__", Value::Builtin(b("Object", "create")));
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
        self.set_prop(
            arr_proto_id,
            "Symbol.iterator",
            Value::Builtin(b("Array", "values")),
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

        let typed_array_proto_id = self.alloc_object();
        self.set_prop(
            typed_array_proto_id,
            "filter",
            Value::Builtin(b("Array", "filter")),
        );
        let typed_array_ctor_id = self.alloc_object();
        self.set_prop(
            typed_array_ctor_id,
            "prototype",
            Value::Object(typed_array_proto_id),
        );
        self.set_prop(
            typed_array_proto_id,
            "constructor",
            Value::Object(typed_array_ctor_id),
        );
        self.typed_array_constructor_id = Some(typed_array_ctor_id);
        self.typed_array_prototype_id = Some(typed_array_proto_id);
        self.set_prop(global_id, "TypedArray", Value::Object(typed_array_ctor_id));

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
            "matchAll",
            Value::Builtin(b("String", "matchAll")),
        );
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
            Value::Builtin(b("String", "replaceAll")),
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
        self.set_prop(
            str_proto_id,
            "charCodeAt",
            Value::Builtin(b("String", "charCodeAt")),
        );
        self.set_prop(
            str_proto_id,
            "codePointAt",
            Value::Builtin(b("String", "codePointAt")),
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
            "substring",
            Value::Builtin(b("String", "substring")),
        );
        self.set_prop(
            str_proto_id,
            "substr",
            Value::Builtin(b("String", "substr")),
        );
        self.set_prop(str_proto_id, "sup", Value::Builtin(b("String", "sup")));
        self.set_prop(
            str_proto_id,
            "trimLeft",
            Value::Builtin(b("String", "trimStart")),
        );
        self.set_prop(
            str_proto_id,
            "trimStart",
            Value::Builtin(b("String", "trimStart")),
        );
        self.set_prop(
            str_proto_id,
            "trimRight",
            Value::Builtin(b("String", "trimEnd")),
        );
        self.set_prop(
            str_proto_id,
            "trimEnd",
            Value::Builtin(b("String", "trimEnd")),
        );
        let str_id = self.alloc_object();
        self.set_prop(str_id, "prototype", Value::Object(str_proto_id));
        self.set_prop(str_id, "__call__", Value::Builtin(b("Type", "String")));
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
        self.set_prop(
            ref_err_id,
            "__call__",
            Value::Builtin(b("Error", "ReferenceError")),
        );
        self.set_prop(global_id, "ReferenceError", Value::Object(ref_err_id));

        let type_err_id = self.alloc_object();
        self.set_prop(type_err_id, "name", Value::String("TypeError".to_string()));
        self.set_prop(
            type_err_id,
            "__call__",
            Value::Builtin(b("Error", "TypeError")),
        );
        self.set_prop(global_id, "TypeError", Value::Object(type_err_id));

        let range_err_id = self.alloc_object();
        self.set_prop(
            range_err_id,
            "name",
            Value::String("RangeError".to_string()),
        );
        self.set_prop(
            range_err_id,
            "__call__",
            Value::Builtin(b("Error", "RangeError")),
        );
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
            Value::Builtin(b("Error", "SyntaxError")),
        );
        self.set_prop(global_id, "SyntaxError", Value::Object(syntax_err_id));

        let uri_err_id = self.alloc_object();
        self.set_prop(uri_err_id, "name", Value::String("URIError".to_string()));
        self.set_prop(
            uri_err_id,
            "__call__",
            Value::Builtin(b("Error", "URIError")),
        );
        self.set_prop(global_id, "URIError", Value::Object(uri_err_id));

        let eval_err_id = self.alloc_object();
        self.set_prop(eval_err_id, "name", Value::String("EvalError".to_string()));
        self.set_prop(
            eval_err_id,
            "__call__",
            Value::Builtin(b("Error", "EvalError")),
        );
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

        let error_proto_id = self.alloc_object();
        self.set_prop(err_id, "prototype", Value::Object(error_proto_id));

        let reference_error_proto_id = self.alloc_object();
        self.set_prototype(reference_error_proto_id, Some(error_proto_id));
        self.set_prop(
            ref_err_id,
            "prototype",
            Value::Object(reference_error_proto_id),
        );

        let type_error_proto_id = self.alloc_object();
        self.set_prototype(type_error_proto_id, Some(error_proto_id));
        self.set_prop(type_err_id, "prototype", Value::Object(type_error_proto_id));

        let range_error_proto_id = self.alloc_object();
        self.set_prototype(range_error_proto_id, Some(error_proto_id));
        self.set_prop(
            range_err_id,
            "prototype",
            Value::Object(range_error_proto_id),
        );

        let syntax_error_proto_id = self.alloc_object();
        self.set_prototype(syntax_error_proto_id, Some(error_proto_id));
        self.set_prop(
            syntax_err_id,
            "prototype",
            Value::Object(syntax_error_proto_id),
        );

        let uri_error_proto_id = self.alloc_object();
        self.set_prototype(uri_error_proto_id, Some(error_proto_id));
        self.set_prop(uri_err_id, "prototype", Value::Object(uri_error_proto_id));

        let eval_error_proto_id = self.alloc_object();
        self.set_prototype(eval_error_proto_id, Some(error_proto_id));
        self.set_prop(eval_err_id, "prototype", Value::Object(eval_error_proto_id));

        let aggregate_error_proto_id = self.alloc_object();
        self.set_prototype(aggregate_error_proto_id, Some(error_proto_id));
        self.set_prop(
            aggregate_err_id,
            "prototype",
            Value::Object(aggregate_error_proto_id),
        );

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
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren1",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren2",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren3",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren4",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren5",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren6",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren7",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren8",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_paren9",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_input",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_last_match",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_last_paren",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_left_context",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "__legacy_regexp_right_context",
            Value::String(String::new()),
        );
        self.set_prop(
            regexp_id,
            "$1",
            Value::Builtin(b("RegExp", "legacy_get_paren1")),
        );
        self.set_prop(
            regexp_id,
            "$2",
            Value::Builtin(b("RegExp", "legacy_get_paren2")),
        );
        self.set_prop(
            regexp_id,
            "$3",
            Value::Builtin(b("RegExp", "legacy_get_paren3")),
        );
        self.set_prop(
            regexp_id,
            "$4",
            Value::Builtin(b("RegExp", "legacy_get_paren4")),
        );
        self.set_prop(
            regexp_id,
            "$5",
            Value::Builtin(b("RegExp", "legacy_get_paren5")),
        );
        self.set_prop(
            regexp_id,
            "$6",
            Value::Builtin(b("RegExp", "legacy_get_paren6")),
        );
        self.set_prop(
            regexp_id,
            "$7",
            Value::Builtin(b("RegExp", "legacy_get_paren7")),
        );
        self.set_prop(
            regexp_id,
            "$8",
            Value::Builtin(b("RegExp", "legacy_get_paren8")),
        );
        self.set_prop(
            regexp_id,
            "$9",
            Value::Builtin(b("RegExp", "legacy_get_paren9")),
        );
        self.set_prop(
            regexp_id,
            "input",
            Value::Builtin(b("RegExp", "legacy_get_input")),
        );
        self.set_prop(
            regexp_id,
            "$_",
            Value::Builtin(b("RegExp", "legacy_get_input")),
        );
        self.set_prop(
            regexp_id,
            "lastMatch",
            Value::Builtin(b("RegExp", "legacy_get_last_match")),
        );
        self.set_prop(
            regexp_id,
            "$&",
            Value::Builtin(b("RegExp", "legacy_get_last_match")),
        );
        self.set_prop(
            regexp_id,
            "lastParen",
            Value::Builtin(b("RegExp", "legacy_get_last_paren")),
        );
        self.set_prop(
            regexp_id,
            "$+",
            Value::Builtin(b("RegExp", "legacy_get_last_paren")),
        );
        self.set_prop(
            regexp_id,
            "leftContext",
            Value::Builtin(b("RegExp", "legacy_get_left_context")),
        );
        self.set_prop(
            regexp_id,
            "$`",
            Value::Builtin(b("RegExp", "legacy_get_left_context")),
        );
        self.set_prop(
            regexp_id,
            "rightContext",
            Value::Builtin(b("RegExp", "legacy_get_right_context")),
        );
        self.set_prop(
            regexp_id,
            "$'",
            Value::Builtin(b("RegExp", "legacy_get_right_context")),
        );
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
        let sym_unscopables = self.alloc_symbol(Some("Symbol.unscopables".to_string()));
        let sym_dispose = self.alloc_symbol(Some("Symbol.dispose".to_string()));
        let symbol_id = self.alloc_object();
        self.set_prop(symbol_id, "__call__", Value::Builtin(b("Symbol", "create")));
        self.set_prop(symbol_id, "for", Value::Builtin(b("Symbol", "for")));
        self.set_prop(symbol_id, "keyFor", Value::Builtin(b("Symbol", "keyFor")));
        self.set_prop(symbol_id, "match", Value::Symbol(sym_match));
        self.set_prop(symbol_id, "replace", Value::Symbol(sym_replace));
        self.set_prop(symbol_id, "search", Value::Symbol(sym_search));
        self.set_prop(symbol_id, "split", Value::Symbol(sym_split));
        self.set_prop(symbol_id, "iterator", Value::Symbol(sym_iterator));
        self.set_prop(symbol_id, "species", Value::Symbol(sym_species));
        self.set_prop(symbol_id, "toStringTag", Value::Symbol(sym_to_string_tag));
        self.set_prop(symbol_id, "unscopables", Value::Symbol(sym_unscopables));
        self.set_prop(symbol_id, "dispose", Value::Symbol(sym_dispose));
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
        let array_buffer_proto_id = self.alloc_object();
        self.set_prop(
            array_buffer_proto_id,
            "resize",
            Value::Builtin(b("TypedArray", "ArrayBufferResize")),
        );
        self.array_buffer_prototype_id = Some(array_buffer_proto_id);
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
        for constructor_name in [
            "Object",
            "Array",
            "String",
            "Number",
            "Error",
            "ReferenceError",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "URIError",
            "EvalError",
            "AggregateError",
            "RegExp",
            "Date",
            "Function",
            "Promise",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "Proxy",
            "Symbol",
        ] {
            if let Value::Object(ctor_id) = self.get_prop(global_id, constructor_name) {
                self.set_prototype(ctor_id, Some(func_proto_id));
            }
        }
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
        self.set_prop(reflect_id, "set", Value::Builtin(b("Reflect", "set")));
        self.set_prop(reflect_id, "apply", Value::Builtin(b("Reflect", "apply")));
        self.set_prop(
            reflect_id,
            "construct",
            Value::Builtin(b("Reflect", "construct")),
        );
        self.set_prop(
            reflect_id,
            "defineProperty",
            Value::Builtin(b("Reflect", "defineProperty")),
        );
        self.set_prop(
            reflect_id,
            "getOwnPropertyDescriptor",
            Value::Builtin(b("Reflect", "getOwnPropertyDescriptor")),
        );
        self.set_prop(reflect_id, "has", Value::Builtin(b("Reflect", "has")));
        self.set_prop(
            reflect_id,
            "deleteProperty",
            Value::Builtin(b("Reflect", "deleteProperty")),
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

        self.set_prop(global_id, "BigInt", Value::Builtin(b("Global", "BigInt")));
        self.set_prop(
            global_id,
            "SharedArrayBuffer",
            Value::Builtin(b("Global", "SharedArrayBuffer")),
        );

        let iterator_id = self.alloc_object();
        self.set_prop(iterator_id, "from", Value::Builtin(b("Iterator", "from")));
        self.set_prop(iterator_id, "name", Value::String("Iterator".to_string()));
        self.set_prop(iterator_id, "length", Value::Int(0));
        let iterator_proto_id = self.alloc_object();
        for helper_name in [
            "map", "filter", "take", "drop", "flatMap", "reduce", "toArray", "forEach", "some",
            "every", "find",
        ] {
            self.set_prop(
                iterator_proto_id,
                helper_name,
                Value::Builtin(b("Iterator", "from")),
            );
        }
        self.set_prop(
            iterator_proto_id,
            "Symbol.iterator",
            Value::Builtin(b("Iterator", "prototypeIterator")),
        );
        self.set_prop(
            iterator_proto_id,
            "Symbol.dispose",
            Value::Builtin(b("Iterator", "prototypeDispose")),
        );
        self.set_prop(
            iterator_proto_id,
            "Symbol.toStringTag",
            Value::String("Iterator".to_string()),
        );
        self.set_prop(iterator_proto_id, "constructor", Value::Object(iterator_id));
        self.set_prop(iterator_id, "prototype", Value::Object(iterator_proto_id));
        self.set_prop(global_id, "Iterator", Value::Object(iterator_id));

        let atomics_id = self.alloc_object();
        for method_name in [
            "add",
            "and",
            "compareExchange",
            "exchange",
            "isLockFree",
            "load",
            "or",
            "store",
            "sub",
            "wait",
            "waitAsync",
            "notify",
            "xor",
        ] {
            self.set_prop(atomics_id, method_name, Value::Builtin(b("Atomics", "op")));
        }
        self.set_prop(global_id, "Atomics", Value::Object(atomics_id));
        self.set_prop(
            global_id,
            "DisposableStack",
            Value::Builtin(b("Global", "DisposableStack")),
        );
        self.set_prop(
            global_id,
            "SuppressedError",
            Value::Builtin(b("Global", "SuppressedError")),
        );

        for constructor_name in [
            "Object",
            "Array",
            "String",
            "Number",
            "Error",
            "ReferenceError",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "URIError",
            "EvalError",
            "AggregateError",
            "RegExp",
            "Date",
            "Function",
            "Promise",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "Proxy",
            "Symbol",
        ] {
            if let Value::Object(ctor_id) = self.get_prop(global_id, constructor_name) {
                self.set_prototype(ctor_id, Some(func_proto_id));
            }
        }
    }

    pub fn init_compat_globals(&mut self) {
        let global_id = self.global_object_id;
        let require_builtin = builtins::resolve("Compat", "require").expect("Compat.require");
        self.set_prop(global_id, "require", Value::Builtin(require_builtin));
        let process_id = self.alloc_object();
        let env_id = self.alloc_object();
        self.set_prop(process_id, "env", Value::Object(env_id));
        self.set_prop(global_id, "process", Value::Object(process_id));
        self.set_prop(global_id, "global", Value::Object(global_id));
        self.set_prop(global_id, "self", Value::Object(global_id));
        let exports_id = self.alloc_object();
        let module_id = self.alloc_object();
        self.set_prop(module_id, "exports", Value::Object(exports_id));
        self.set_prop(global_id, "module", Value::Object(module_id));
        self.set_prop(global_id, "exports", Value::Object(exports_id));
        self.set_prop(global_id, "__dirname", Value::String(".".to_string()));
        self.set_prop(
            global_id,
            "__filename",
            Value::String("script.js".to_string()),
        );
    }

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
}
