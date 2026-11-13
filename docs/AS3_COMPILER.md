# AS3 compiler subset

`nova-as` contains a real ActionScript 3 front-end and AVM2 back-end. It is not a
parser stub and it does not translate AS3 to JavaScript. On export, every project
script is compiled to ABC bytecode; if any script fails, the SWF build fails.

The compiler is deliberately conservative: **a construct is either lowered to
correct ABC or reported as an error with its source file, line and column.** No
unsupported construct is silently ignored or approximated.

## Pipeline

```text
source text
  → lexer            nova-as/src/lexer.rs
  → parser / AST     nova-as/src/parser.rs, ast.rs
  → symbol creation  compiler/check.rs, compiler/types.rs
  → type checking    compiler/check.rs
  → ABC emitter      compiler/emit.rs
  → serializer       compiler/abc.rs
  → DoABC2 in SWF    nova-build/src/lib.rs
```

`nova_as::compile(source)` returns a `CompiledUnit`:

- `abc`: complete ABC bytes, or empty when there is any error;
- `diagnostics`: errors and warnings with `Span` and severity;
- `classes`: classes that were emitted, including qualified name, base class,
  whether the class extends `MovieClip`, whether it has a constructor and its
  source line.

`compile_with_options` additionally accepts a package override and a main-class
selection (`CompileOptions`).

## Supported declarations

- One package block (including the default package), with multiple classes.
- Imports, including wildcard imports. Imports are used for name resolution and
  to build dynamic multinames for external Flash classes. They do **not** load or
  validate a `playerglobal` declaration file.
- Classes, single inheritance with `extends`, and package/default names.
- Access modifiers `public`, `private`, `protected`, `internal`; `static`;
  `final`; `override` on methods. Access modifiers are emitted as AVM2
  namespaces. Full compile-time access control (for example rejecting access to
  another class's private member) is not complete.
- Instance and static fields declared with `var` or `const`, optional type
  annotation and optional initializer.
- Instance and static methods, constructors, default parameter values, `return`.
- Top-level package functions used as free functions.
- Types `int`, `uint`, `Number`, `Boolean`, `String`, `Object`, `void`, `*`,
  declared classes, imported/dotted external classes, and local class types.
  `Array`, `Vector`, `Function` and function types are rejected.

Constructors are special:

- If a class has no constructor, a default constructor is synthesized.
- The constructor is emitted as `InstanceInfo.init_method`; there is no separate
  `iinit` method and no constructor trait.
- The generated prologue is `getlocal0; pushscope; getlocal0; constructsuper 0`.
  Instance field initializers run next, then the user constructor body.
- A base class whose constructor requires arguments cannot be called because
  explicit `super(...)` is not implemented. The checker reports this for local
  base classes. External bases are assumed to accept zero arguments.

## Supported statements

- Blocks and empty statements
- Local `var` / `const`
- Expression statements
- `if` / `else`
- `while`
- `do ... while`
- `for` with var or expression initializer, optional condition and update
- `return`
- `break` and `continue`

## Supported expressions

- Numeric, string, `true`, `false`, `null`, `undefined`, `NaN`, `Infinity`
- `this`, identifiers, property access (`a.b`), calls (`f(...)`, `a.f(...)`)
- `new ClassName(...)`
- Assignment `=`, `+=`, `-=`, `*=`, `/=`, `%=`, bitwise and shift assignments
- Prefix and postfix `++` / `--`
- Unary `+`, `-`, `!`, `~`, `typeof`, `void`
- Arithmetic `+ - * / %`
- Comparison `< <= > >=`, equality `== != === !==`
- Logical `&& ||` with short-circuit lowering
- Bitwise `& | ^`, shifts `<< >> >>>`
- `instanceof`, `is`, `as`
- Conditional expression `condition ? a : b`
- Parenthesized expressions

Calls support:

- methods on `this` and on typed values;
- static methods and properties on known or imported classes;
- constructors of known classes;
- declared package functions;
- a small set of AVM2 global functions: `trace`, `getTimer`, `isNaN`,
  `isFinite`, `parseInt`, `parseFloat` (see below);
- inherited external methods such as `addEventListener`: these are emitted as
  dynamic calls on `this`.

Calling a function stored in a local, field, parameter or other function value is
rejected with a compiler error. Passing a method as a value (for example
`addEventListener(Event.ENTER_FRAME, update)`) is allowed; only *calling* through
an arbitrary function value is not.

## Built-in and external APIs

The compiler recognizes these free functions as members of the AVM2 global
object:

| Function | Result |
| --- | --- |
| `trace(...)` | void |
| `getTimer()` | int |
| `isNaN(value)` | Boolean |
| `isFinite(value)` | Boolean |
| `parseInt(text, radix)` | Number |
| `parseFloat(text)` | Number |

Everything else from `flash.*` is treated as an external class surface. This is
intentional: the projector supplies the implementations, while the compiler only
needs a multiname and a plausible type. Examples:

```actionscript
import flash.display.MovieClip;
import flash.events.Event;

public class Box extends MovieClip {
    public function Box() {
        addEventListener(Event.ENTER_FRAME, update);
    }

    private function update(event:Event):void {
        x += 2;
        rotation += 0.5;
    }
}
```

`Event.ENTER_FRAME` and `addEventListener` are dynamic accesses. `x` and
`rotation` are inherited display-object properties, also dynamic. This compiles
and links, but the compiler cannot type-check the full `MovieClip` API. A
dedicated `playerglobal` declaration layer is the next API milestone; see
[docs/ROADMAP.md](ROADMAP.md).

## Explicitly unsupported

The following are rejected rather than mistranslated:

- `interface`, `implements`, `dynamic`, `native`, getters/setters
- explicit `super(...)` calls and `super.method()`
- `switch`, `try` / `catch` / `finally`, `with`, `throw`
- `for in`, `for each`, labels
- array literals and index access (`a[i]`)
- object literals
- `delete`, `in`
- calls through function values, closures, nested functions
- `Array`, `Vector`, `Function`, generics, E4X
- metadata annotations such as `[Event(...)]` (the ABC metadata array is
  serialized empty)
- classes with a base constructor that requires arguments
- array/function parameter and field types

Some of these are parsed so the diagnostic can point at the exact construct;
others are rejected by the parser before type checking. In both cases no ABC is
emitted.

## Diagnostics

Errors render like a normal compiler:

```text
scripts/Main.as:4:18: Fehler: Typ 'String' kann nicht 'int' zugewiesen werden
            var x:int = "nope";
                        ^
```

The build pipeline (`nova_build`) prefixes the script path automatically. A
single error aborts the SWF export with `BuildError::ActionScript`; there is no
partial or empty ABC embedded. Warnings are returned with the build output and
shown in the editor.

## Linking scripts to content

Scripts are project files. They can be associated in two ways:

1. `ObjectInstance.script` on an instance; every visible instance of that object
   receives the class.
2. `Script.linked_object`, which associates the script with one object instance
   without editing the object.

A script is also a **document-class candidate** if it is not linked to an object
and contains exactly one class whose name is the script stem or `Main`, and that
class extends `MovieClip` or `Sprite`. That class is linked to character 0.

Each linked object is wrapped in a one-frame `DefineSprite` and linked through
`SymbolClass`. This lets the class extend `MovieClip`/`Sprite`, receive
`ENTER_FRAME` events and control its own display properties. After the first
placement, the SWF timeline does not re-place the object; ActionScript owns its
transform. See [SWF_EXPORT.md](SWF_EXPORT.md).

## Test evidence

- Unit tests cover lexer/parser/checker behavior and opcode encoding.
- `crates/nova-as/tests/compiler.rs` compiles classes with fields, static
  members, constructors, loops, branches, calls, `new`, inherited external
  methods and `trace`. The resulting ABC is parsed with Ruffle's independent
  `swf::avm2::read::Reader`.
- `crates/nova-build/tests/swf_scripts.rs` builds complete projects and parses
  `DoABC2`, `SymbolClass`, the ABC payload and the sprite wrappers independently.
- `samples/as3-example.nova` is a checked-in project with a class, constructor,
  fields and an `ENTER_FRAME` update method; a test builds it on every run.

These tests prove structure, not execution. No AVM2 runtime or Flash Player
Projector is available in this environment.
