use nova_as::{
    CompileOptions, CompiledClass, CompiledUnit, Severity, compile, compile_with_options,
};
use swf::avm2::read::Reader;
use swf::avm2::types::{AbcFile, Multiname, Namespace};

// AVM2 opcodes used by the emitted initialization sequences (swf keeps its
// `OpCode` enum private, so the byte values are spelled out here).
const GET_LOCAL0: u8 = 0xd0;
const PUSH_SCOPE: u8 = 0x30;
const CONSTRUCT_SUPER: u8 = 0x49;
const RETURN_VOID: u8 = 0x47;
const PUSH_BYTE: u8 = 0x24;
const COERCE: u8 = 0x80;
const GET_SCOPE_OBJECT: u8 = 0x65;
const FIND_PROP_STRICT: u8 = 0x5d;
const NEW_CLASS: u8 = 0x58;
const INIT_PROPERTY: u8 = 0x68;
const FIND_PROPERTY: u8 = 0x5e;

fn parse_abc(unit: &CompiledUnit) -> AbcFile {
    assert!(
        !unit.has_errors(),
        "compilation failed: {:#?}",
        unit.diagnostics
    );
    assert!(!unit.abc.is_empty(), "no ABC emitted");
    Reader::new(&unit.abc)
        .read()
        .expect("generated ABC must parse")
}

fn string<'a>(abc: &'a AbcFile, index: swf::avm2::types::Index<String>) -> &'a str {
    let index = index.as_u30();
    if index == 0 {
        return "";
    }
    std::str::from_utf8(&abc.constant_pool.strings[index as usize - 1]).unwrap()
}

fn namespace_name(abc: &AbcFile, index: swf::avm2::types::Index<Namespace>) -> &str {
    let name = match abc.constant_pool.namespaces[index.as_u30() as usize - 1] {
        Namespace::Namespace(name)
        | Namespace::Package(name)
        | Namespace::PackageInternal(name)
        | Namespace::Protected(name)
        | Namespace::Explicit(name)
        | Namespace::StaticProtected(name)
        | Namespace::Private(name) => name,
    };
    string(abc, name)
}

fn multiname(abc: &AbcFile, index: swf::avm2::types::Index<Multiname>) -> (&str, &str) {
    match &abc.constant_pool.multinames[index.as_u30() as usize - 1] {
        Multiname::QName { namespace, name } => {
            (namespace_name(abc, *namespace), string(abc, *name))
        }
        Multiname::Multiname {
            namespace_set: _,
            name,
        } => ("*", string(abc, *name)),
        other => panic!("unexpected multiname {other:?}"),
    }
}

fn classes(unit: &CompiledUnit) -> Vec<&CompiledClass> {
    unit.classes.iter().collect()
}

fn method_body<'a>(abc: &'a AbcFile, name: &str) -> &'a swf::avm2::types::MethodBody {
    let method_index = abc
        .methods
        .iter()
        .position(|method| string(abc, method.name) == name)
        .unwrap_or_else(|| panic!("method {name} not found"));
    let body_index = abc.methods[method_index]
        .body
        .unwrap_or_else(|| panic!("method {name} has no body"));
    &abc.method_bodies[body_index.as_u30() as usize]
}

#[test]
fn compiles_a_simple_class_into_parsable_abc() {
    let source = r#"package {
    public class Counter {
        public var count:int = 0;

        public function Counter(start:int) {
            count = start;
        }

        public function increment(step:int):int {
            count += step;
            return count;
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.abc.is_empty(), "diagnostics: {:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    assert_eq!(abc.minor_version, 16);
    assert_eq!(abc.major_version, 46);
    assert_eq!(abc.instances.len(), 1);
    assert_eq!(abc.classes.len(), 1);
    assert_eq!(abc.scripts.len(), 1);
    assert_eq!(classes(&unit).len(), 1);

    let instance = &abc.instances[0];
    assert_eq!(multiname(&abc, instance.name), ("", "Counter"));
    assert_eq!(multiname(&abc, instance.super_name), ("", "Object"));
    assert!(instance.is_sealed);

    let qnames: Vec<(&str, &str)> = abc.scripts[0]
        .traits
        .iter()
        .filter_map(|trait_| match trait_.kind {
            swf::avm2::types::TraitKind::Class { .. } => Some(multiname(&abc, trait_.name)),
            _ => None,
        })
        .collect();
    assert_eq!(qnames, vec![("", "Counter")]);
}

#[test]
fn emits_adobe_style_initializer_and_script_shapes() {
    let source = r#"package {
    public class Shape {
        public var value:int = 3;
        public static var counter:int = 1;

        public function Shape() {}

        public static function make():int {
            return 1;
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);

    let constructor = method_body(&abc, "Shape");
    assert_eq!(
        (constructor.init_scope_depth, constructor.max_scope_depth),
        (4, 5)
    );
    assert_eq!(
        &constructor.code[..5],
        &[GET_LOCAL0, PUSH_SCOPE, GET_LOCAL0, CONSTRUCT_SUPER, 0,],
        "unexpected constructor bytecode: {:02x?}",
        constructor.code
    );
    assert_eq!(
        &constructor.code[5..9],
        &[GET_LOCAL0, PUSH_BYTE, 3, COERCE],
        "instance field initializer missing: {:02x?}",
        constructor.code
    );
    assert_eq!(constructor.code[10], INIT_PROPERTY);
    assert_eq!(constructor.code.last().copied(), Some(RETURN_VOID));

    let cinit = abc
        .method_bodies
        .iter()
        .find(|body| body.init_scope_depth == 3 && body.max_scope_depth == 4)
        .expect("class initializer");
    assert_eq!(
        &cinit.code[..3],
        &[GET_LOCAL0, PUSH_SCOPE, FIND_PROPERTY],
        "unexpected class initializer bytecode: {:02x?}",
        cinit.code
    );
    // Adobe initializes static fields with findproperty/setproperty.
    const SET_PROPERTY: u8 = 0x61;
    assert!(cinit.code.contains(&SET_PROPERTY));
    assert!(!cinit.code.contains(&INIT_PROPERTY));
    assert_eq!(cinit.code.last().copied(), Some(RETURN_VOID));

    let static_method = method_body(&abc, "make");
    assert_eq!(
        (
            static_method.init_scope_depth,
            static_method.max_scope_depth
        ),
        (3, 4)
    );
    assert_eq!(
        &static_method.code[..2],
        &[GET_LOCAL0, PUSH_SCOPE],
        "static method bytecode: {:02x?}",
        static_method.code
    );

    let script = abc
        .method_bodies
        .iter()
        .find(|body| body.init_scope_depth == 1 && body.max_scope_depth == 3)
        .expect("script initializer");
    assert_eq!(
        &script.code[..3],
        &[GET_LOCAL0, PUSH_SCOPE, GET_SCOPE_OBJECT,],
        "unexpected script initializer bytecode: {:02x?}",
        script.code
    );
    assert!(script.code.contains(&(NEW_CLASS)));
    assert!(script.code.contains(&(INIT_PROPERTY)));
}

#[test]
fn emits_instance_and_static_traits_and_optional_defaults() {
    let source = r#"package pkg {
    public class Fields {
        public var instanceValue:int;
        private static const LIMIT:int = 7;

        public function Fields() {}

        public function mix(a:int = 1, b:Number = 2.5, c:String = "hi", d:Boolean = true):void {}
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);

    let instance = &abc.instances[0];
    let instance_slots: Vec<(&str, &str)> = instance
        .traits
        .iter()
        .filter_map(|trait_| match trait_.kind {
            swf::avm2::types::TraitKind::Slot { .. }
            | swf::avm2::types::TraitKind::Const { .. } => Some(multiname(&abc, trait_.name)),
            _ => None,
        })
        .collect();
    assert_eq!(instance_slots, vec![("pkg", "instanceValue")]);

    let class = &abc.classes[0];
    assert_eq!(class.traits.len(), 1);
    let (private_namespace, class_field) = multiname(&abc, class.traits[0].name);
    assert_eq!(class_field, "LIMIT");
    assert!(private_namespace.ends_with("Fields"));
    assert!((class.init_method.as_u30() as usize) < abc.methods.len());
    assert!(
        abc.methods[class.init_method.as_u30() as usize]
            .body
            .is_some()
    );

    let method_index = abc
        .methods
        .iter()
        .position(|method| string(&abc, method.name) == "mix")
        .expect("mix method");
    let method = &abc.methods[method_index];
    assert_eq!(method.params.len(), 4);
    match method.params[0].default_value {
        Some(swf::avm2::types::DefaultValue::Int(index)) => {
            assert_eq!(abc.constant_pool.ints[index.as_u30() as usize - 1], 1);
        }
        ref other => panic!("unexpected int default: {other:?}"),
    }
    assert!(matches!(
        method.params[1].default_value,
        Some(swf::avm2::types::DefaultValue::Double(_))
    ));
    assert!(matches!(
        method.params[2].default_value,
        Some(swf::avm2::types::DefaultValue::String(_))
    ));
    assert_eq!(
        method.params[3].default_value,
        Some(swf::avm2::types::DefaultValue::True)
    );
    assert!(method.body.is_some());
}

#[test]
fn compiles_calls_new_static_and_external_members() {
    let source = r#"package pkg {
    import flash.display.MovieClip;
    import flash.events.Event;

    public class Main extends MovieClip {
        private static var instances:int = 0;

        public function Main() {
            instances = Main.twice(2);
            addEventListener(Event.ENTER_FRAME, update);
            var helper:Helper = new Helper();
        }

        public static function twice(n:int):int {
            return n * 2;
        }

        private function update(event:Event):void {
            this.alpha = 0.5;
        }
    }

    public class Helper {
        public function Helper() {}
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    assert!(!abc.method_bodies.is_empty());
    assert!(
        abc.instances[0]
            .traits
            .iter()
            .any(|trait_| matches!(trait_.kind, swf::avm2::types::TraitKind::Method { .. }))
    );
    assert!(
        abc.classes[0]
            .traits
            .iter()
            .any(|trait_| matches!(trait_.kind, swf::avm2::types::TraitKind::Method { .. }))
    );
    assert!(
        abc.classes[0]
            .traits
            .iter()
            .any(|trait_| matches!(trait_.kind, swf::avm2::types::TraitKind::Slot { .. }))
    );
}

#[test]
fn compiles_movieclip_subclass_and_reports_metadata() {
    let source = r#"package {
    import flash.display.MovieClip;

    public class Main extends MovieClip {
        public function Main() {
            trace("hello");
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    let instance = &abc.instances[0];
    assert_eq!(
        multiname(&abc, instance.super_name),
        ("flash.display", "MovieClip")
    );
    let compiled = classes(&unit);
    assert_eq!(compiled[0].name, "Main");
    assert_eq!(compiled[0].qualified_name, "Main");
    assert_eq!(compiled[0].base.as_deref(), Some("flash.display.MovieClip"));
    assert!(compiled[0].extends_movieclip);
    assert!(compiled[0].constructor);
    assert_eq!(classes(&unit)[0].source_line, 4);

    // `trace` is a free function on the playerglobal object. The receiver
    // must come from `findpropstrict`, which resolves through the scope
    // chain; `getglobalscope`/`getscopeobject` would return the wrong object
    // and `callproperty` would fail with "value is not a function".
    let constructor = method_body(&abc, "Main");
    assert!(
        constructor.code.contains(&FIND_PROP_STRICT),
        "global receiver missing: {:02x?}",
        constructor.code
    );
}

#[test]
fn resolves_external_inherited_members_through_base_packages() {
    let source = r#"package {
    public class Main extends flash.display.MovieClip {
        public function Main() {
            gotoAndStop(1);
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    let namespace_set = abc
        .constant_pool
        .multinames
        .iter()
        .find_map(|multiname| match multiname {
            Multiname::Multiname {
                name,
                namespace_set,
            } if string(&abc, *name) == "gotoAndStop" => Some(*namespace_set),
            _ => None,
        })
        .expect("dynamic multiname for gotoAndStop");
    let index = namespace_set.as_u30() as usize;
    assert!(index > 0);
    let names: Vec<&str> = abc.constant_pool.namespace_sets[index - 1]
        .iter()
        .map(|namespace| namespace_name(&abc, *namespace))
        .collect();
    assert!(
        names.contains(&"flash.display"),
        "base package missing from namespace set: {names:?}"
    );
}

#[test]
fn reports_movieclip_ancestors_through_local_base_classes() {
    let source = r#"package {
    import flash.display.MovieClip;

    public class Base extends MovieClip {}
    public class Child extends Base {}
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let compiled = classes(&unit);
    let base = compiled
        .iter()
        .find(|class| class.name == "Base")
        .expect("Base");
    let child = compiled
        .iter()
        .find(|class| class.name == "Child")
        .expect("Child");
    assert!(base.extends_movieclip);
    assert!(child.extends_movieclip);
    assert_eq!(child.base.as_deref(), Some("Base"));
}

#[test]
fn returns_no_abc_on_checker_errors() {
    let unit = compile("class Broken { public function f():int { missing(); return 1; } }");
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(unit.classes.is_empty());
}

#[test]
fn reports_source_lines_for_errors() {
    let unit = compile(
        "package {\n  public class A {\n    public function A() {\n      unknown = 1;\n    }\n  }\n}\n",
    );
    assert!(unit.abc.is_empty());
    let error = unit
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == Severity::Error)
        .expect("error");
    assert!(error.message.contains("unbekannt") || error.message.contains("Variable"));
    assert_eq!(error.span.line, 4);
}

#[test]
fn options_override_package_and_select_main_class() {
    let source = r#"package old.pkg {
    public class A {}
    public class B {}
}
"#;
    let unit = compile_with_options(
        source,
        CompileOptions {
            package: Some("new.pkg".into()),
            main_class: Some("B".into()),
        },
    );
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    assert_eq!(multiname(&abc, abc.instances[0].name), ("new.pkg", "B"));
    assert_eq!(multiname(&abc, abc.instances[1].name), ("new.pkg", "A"));
}

#[test]
fn unknown_main_class_is_an_error_without_abc() {
    let unit = compile_with_options(
        "class A {}",
        CompileOptions {
            package: None,
            main_class: Some("Nope".into()),
        },
    );
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
}

#[test]
fn supports_loops_conditionals_and_prefix_increment() {
    let source = r#"package {
    public class Loop {
        public function run(n:int):int {
            var total:int = 0;
            var i:int = 0;
            while (i < n) {
                ++i;
                if (i % 2 == 0) {
                    continue;
                }
                total += i;
            }
            for (var j:int = 0; j < n; j++) {
                total -= 1;
            }
            do {
                total++;
            } while (total < 0);
            return total;
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    assert!(!abc.method_bodies.is_empty());
    // Jump patching must produce valid relative branch operands; the reader
    // accepting the file already proves the structure, but ensure branches exist.
    assert!(
        abc.method_bodies
            .iter()
            .any(|body| body.code.contains(&0x10))
    );
}

#[test]
fn patches_branches_relative_to_the_end_of_the_instruction() {
    let source = r#"package {
    public class Branch {
        public function pick(x:int):int {
            if (x > 0) {
                return 1;
            }
            return 0;
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    let body = method_body(&abc, "pick");
    const GET_LOCAL1: u8 = 0xd1;
    const GREATERTHAN: u8 = 0xaf;
    const IFFALSE: u8 = 0x12;
    assert_eq!(
        body.code[..6],
        [
            GET_LOCAL0,
            PUSH_SCOPE,
            GET_LOCAL1,
            PUSH_BYTE,
            0,
            GREATERTHAN
        ]
    );
    assert_eq!(body.code[6], IFFALSE);
    // The branch target is the `pushbyte 0` after `return 1`:
    // target 15 - end-of-branch 10 = 5 (the return value is coerced to int).
    assert_eq!(&body.code[7..10], &[5, 0, 0]);
}

#[test]
fn rejects_unsupported_statements_with_a_line() {
    for keyword in ["switch", "try", "with"] {
        let source = format!("class A {{ public function f():void {{ {keyword} {{ }} }} }}");
        let unit = compile(&source);
        assert!(unit.has_errors(), "{keyword} should be rejected");
        assert!(unit.abc.is_empty());
        assert!(
            unit.diagnostics
                .iter()
                .any(|d| d.message.contains(keyword) && d.severity == Severity::Error)
        );
    }
}

#[test]
fn emits_parsable_array_object_and_index_abc() {
    let unit = compile(
        "class A { public function f():void { \
         var a = [1, 2]; var d = {x: 3}; a[0] = d[\"x\"]; trace(a[0]); } }",
    );
    let abc = parse_abc(&unit);
    assert!(
        abc.constant_pool
            .multinames
            .iter()
            .any(|multiname| matches!(multiname, Multiname::MultinameL { .. })),
        "generated ABC must contain a late-bound multiname"
    );
}

#[test]
fn rejects_for_in_but_accepts_index_access() {
    let unit = compile(
        "class A { public function f(o:Object):void { for (var key in o) { trace(key); } } }",
    );
    assert!(unit.has_errors());
    assert!(
        unit.diagnostics
            .iter()
            .any(|d| d.message.contains("for-in"))
    );

    let unit = compile(
        "class A { public function f(o:Object):void { \
         var a = [1, 2]; var d = {x: 3}; a[0] = d[\"x\"]; trace(a[0]); } }",
    );
    assert!(!unit.has_errors(), "{:?}", unit.diagnostics);
}

#[test]
fn calls_package_functions_through_function_values() {
    const GET_LEX: u8 = 0x60;
    const PUSH_UNDEFINED: u8 = 0x21;
    const CALL: u8 = 0x41;
    const CALL_METHOD: u8 = 0x43;

    let source = r#"package {
    function add(a:int, b:int):int {
        return a + b;
    }

    public class Calc {
        public function run():int {
            return add(1, 2);
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);

    assert!(
        abc.constant_pool
            .multinames
            .iter()
            .any(|multiname| matches!(
                multiname,
                Multiname::QName { name, .. } if string(&abc, *name) == "add"
            )),
        "package function name missing from constant pool"
    );

    let body = method_body(&abc, "run");
    assert!(
        body.code.contains(&GET_LEX),
        "package function must be loaded with getlex: {:02x?}",
        body.code
    );
    assert!(
        body.code.contains(&PUSH_UNDEFINED),
        "package function calls need an undefined receiver: {:02x?}",
        body.code
    );
    assert!(
        body.code.windows(2).any(|window| window == [CALL, 2]),
        "expected call 2: {:02x?}",
        body.code
    );
    assert!(
        !body.code.contains(&CALL_METHOD),
        "callmethod is not valid in user code: {:02x?}",
        body.code
    );
}

#[test]
fn serializes_namespace_set_multinames_in_avm2_order() {
    let source = r#"package {
    public class Sprite {
        public function update(target:Object):void {
            target.alpha = 0.5;
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);

    let namespace_set = abc
        .constant_pool
        .multinames
        .iter()
        .find_map(|multiname| match multiname {
            Multiname::Multiname {
                name,
                namespace_set,
            } if string(&abc, *name) == "alpha" => Some(*namespace_set),
            _ => None,
        })
        .expect("dynamic member multiname for alpha");
    let index = namespace_set.as_u30() as usize;
    assert!(index > 0);
    assert!(index <= abc.constant_pool.namespace_sets.len());
    assert!(!abc.constant_pool.namespace_sets[index - 1].is_empty());
}

#[test]
fn inherited_fields_use_automatic_slot_ids() {
    let source = r#"package {
    public class Base {
        public var baseValue:int;
    }

    public class Child extends Base {
        public var childValue:int;
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    assert_eq!(abc.instances.len(), 2);

    let slot_ids: Vec<u32> = abc.instances[1]
        .traits
        .iter()
        .filter_map(|trait_| match trait_.kind {
            swf::avm2::types::TraitKind::Slot { slot_id, .. }
            | swf::avm2::types::TraitKind::Const { slot_id, .. } => Some(slot_id),
            _ => None,
        })
        .collect();
    assert_eq!(slot_ids, vec![0], "subclass slots must not collide");
}

#[test]
fn supports_is_as_and_instanceof_with_builtin_types() {
    const ASTYPE: u8 = 0x86;
    const ISTYPE: u8 = 0xb2;
    const INSTANCEOF: u8 = 0xb1;

    let source = r#"package {
    public class Coerce {
        public function check(value:Object):Boolean {
            var text:String = value as String;
            if (value instanceof String) {
                return value is String;
            }
            return false;
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    let body = method_body(&abc, "check");
    assert!(body.code.contains(&ASTYPE), "as must use astype");
    assert!(body.code.contains(&ISTYPE), "is must use istype");
    assert!(
        body.code.contains(&INSTANCEOF),
        "instanceof must use instanceof"
    );
}

#[test]
fn rejects_interfaces_with_a_clear_diagnostic() {
    let unit = compile("package { public interface Runner { function run():void; } }");
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(
        unit.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("interface"))
    );
}

#[test]
fn rejects_implicit_super_calls_with_required_arguments() {
    let unit = compile(
        "package { class Base { public function Base(value:int) {} } \
         class Child extends Base { public function Child() {} } }",
    );
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(
        unit.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("super"))
    );
}

#[test]
fn keeps_non_literal_const_fields_const() {
    let source = r#"package {
    public class Constants {
        public static const STATIC_VALUE:int = 1 + 2;
        public const INSTANCE_VALUE:int = 4 + 5;
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);

    let class_trait = abc.classes[0]
        .traits
        .iter()
        .find(|trait_| matches!(trait_.kind, swf::avm2::types::TraitKind::Const { .. }))
        .expect("static const trait");
    match class_trait.kind {
        swf::avm2::types::TraitKind::Const { value, .. } => assert!(value.is_none()),
        _ => unreachable!(),
    }
    let instance_trait = abc.instances[0]
        .traits
        .iter()
        .find(|trait_| matches!(trait_.kind, swf::avm2::types::TraitKind::Const { .. }))
        .expect("instance const trait");
    match instance_trait.kind {
        swf::avm2::types::TraitKind::Const { value, .. } => assert!(value.is_none()),
        _ => unreachable!(),
    }

    const INIT_PROPERTY: u8 = 0x68;
    let cinit = abc
        .method_bodies
        .iter()
        .find(|body| body.init_scope_depth == 3 && body.max_scope_depth == 4)
        .expect("class initializer");
    assert!(cinit.code.contains(&INIT_PROPERTY));
}

#[test]
fn initializes_typed_locals_with_default_values() {
    let source = r#"package {
    public class Defaults {
        public function read():int {
            var value:int;
            return value;
        }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    let body = method_body(&abc, "read");
    const SET_LOCAL1: u8 = 0xd5;
    assert_eq!(
        &body.code[..5],
        &[GET_LOCAL0, PUSH_SCOPE, PUSH_BYTE, 0, SET_LOCAL1],
        "typed local default missing: {:02x?}",
        body.code
    );
}

#[test]
fn requires_and_emits_override_for_inherited_methods() {
    let source = r#"package {
    public class Base {
        public function speak():String { return "base"; }
    }

    public class Child extends Base {
        public override function speak():String { return "child"; }
    }
}
"#;
    let unit = compile(source);
    assert!(!unit.has_errors(), "{:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    let child = abc
        .instances
        .iter()
        .find(|instance| multiname(&abc, instance.name).1 == "Child")
        .expect("Child instance");
    let speak = child
        .traits
        .iter()
        .find(|trait_| multiname(&abc, trait_.name).1 == "speak")
        .expect("speak trait");
    assert!(speak.is_override);
}

#[test]
fn rejects_override_without_inherited_method() {
    let unit = compile("package { public class Lonely { public override function f():void {} } }");
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(
        unit.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("override"))
    );
}

#[test]
fn rejects_missing_override_keyword() {
    let unit = compile(
        "package { class Base { public function f():void {} } \
         class Child extends Base { public function f():void {} } }",
    );
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(
        unit.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("override"))
    );
}

#[test]
fn rejects_subclassing_a_final_class() {
    let unit = compile("package { final class Base {} class Child extends Base {} }");
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(
        unit.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("final"))
    );
}

#[test]
fn rejects_inheritance_cycles() {
    let unit = compile("package { class A extends A {} }");
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(
        unit.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Zyklische"))
    );
}

#[test]
fn rejects_new_with_the_wrong_number_of_arguments() {
    let unit = compile(
        "package { class Pair { public function Pair(a:int, b:int) {} } \
         class Use { public function Use() { var pair:Pair = new Pair(1); } } }",
    );
    assert!(unit.has_errors());
    assert!(unit.abc.is_empty());
    assert!(
        unit.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Argument"))
    );
}

#[test]
fn compiles_switch_with_fallthrough_and_break() {
    let unit = compile(
        "package { public class Main { \
            public function Main() { \
                var x:int = 2; \
                var y:int = 0; \
                switch (x) { \
                    case 1: y = 10; \
                    case 2: y = 20; break; \
                    case 3: y = 30; \
                    default: y = 99; \
                } \
            } \
        } }",
    );
    assert!(!unit.has_errors(), "diagnostics: {:#?}", unit.diagnostics);
    let abc = parse_abc(&unit);
    assert!(!abc.methods.is_empty());
}

#[test]
fn switch_case_expressions_type_check() {
    let unit = compile(
        "package { public class Main { \
            public function Main() { \
                switch (1) { case 1: break; default: break; } \
            } \
        } }",
    );
    assert!(!unit.has_errors(), "diagnostics: {:#?}", unit.diagnostics);
}
