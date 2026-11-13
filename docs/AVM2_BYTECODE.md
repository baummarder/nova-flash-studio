# AVM2 ABC bytecode output

`nova-as` writes a complete ABC (ActionScript Byte Code) block directly. The
result is not AVM1 `DoAction` bytecode and not a serialized syntax tree. The SWF
exporter embeds the bytes in a `DoABC2` tag.

This document describes the layout the emitter produces and the reasoning behind
the unusual parts. It also states what is *not* verified: no AVM2 runtime is
available in this environment, so the bytecode has been validated structurally,
not executed.

## File header

```text
u16 minor = 0x0010 (16)
u16 major = 0x002e (46)
```

Both values are little-endian, as specified for the AVM2 version field. The
minor/major pair matches the AVM2 dialect used by current Flash players. The ABC
reader in Ruffle's `swf` crate accepts the emitted bytes.

## Constant pools

Every pool reserves index 0 and serializes counts including that reserved slot.

| Pool | Contents | Notes |
| --- | --- | --- |
| `int` | `i32` | signed encoded-s32 values |
| `uint` | `u32` | full variable-length encoding, including high bits |
| `double` | `f64` | little-endian IEEE-754 |
| `string` | UTF-8 bytes | length-prefixed with `u30` |
| `namespace` | kind + string index | package, package-internal, private, protected, public |
| `namespace_set` | list of namespace indices | used for dynamic multinames |
| `multiname` | QName or NamespaceSet | QName kind `0x07`, namespace-set kind `0x09` |

The pools deduplicate by value. QNames and namespace sets are cached, and all
`0x09` multinames are written in the AVM2 order `name` then
`namespace_set_index` (the field order differs from QName).

The Any type is index 0. `void` is the public package QName with local name
`void`, not a special constant.

## Methods

`MethodInfo` entries are emitted for:

- each class constructor and synthesized default constructor;
- instance and static methods;
- class initializers (`cinit`);
- package-level functions;
- the script initializer.

Each method has a return type, parameter list, name and flags. Typed parameters
are written with their type index. Default parameter values are written in the
optional-value table; values are supported for numeric, string, boolean and
`null` literals. The `NEED_ARGUMENTS`/`NEED_OPTIONAL` flags are set when
appropriate.

Constructors are represented by `InstanceInfo.init_method`. There is no separate
`iinit` method and no constructor trait. This is the practical AVM2 encoding used
by Flash compilers: constructing an instance invokes the instance initializer.

## Traits and class layout

For every source class the emitter produces:

1. an `InstanceInfo` with the class QName, base QName, sealed/final flags,
   protected namespace, interfaces (currently empty), `init_method`, and
   instance traits;
2. a matching `ClassInfo` in the same order with the class initializer and
   static traits.

Traits use:

- `Slot` for instance/static variables;
- `Const` for `const` fields whose initializer is a literal;
- `Method` for methods;
- `Class` for classes exported by the script;
- `Function` for package-level functions.

Slot IDs are deliberately left at 0 so the AVM2 assigns the next free slot. That
avoids colliding with inherited slots of external bases such as `MovieClip`,
whose layout this compiler does not know.

Access modifiers select namespaces:

| Source | Namespace |
| --- | --- |
| `public` | package namespace of the declaring class |
| `internal` (default) | package-internal namespace |
| `private` | private namespace named after the class |
| `protected` | protected namespace named after the class |

The namespaces are emitted correctly; full compile-time access enforcement is not
complete.

## Initializers and scope depths

ABC method bodies must declare the scope depth at entry and after scopes are
pushed. The emitter uses the following values:

| Body | `init_scope_depth` | `max_scope_depth` |
| --- | ---: | ---: |
| Script initializer | 1 | 3 |
| Global function | 1 | 2 |
| Class initializer (`cinit`) | 3 | 4 |
| Instance method | 4 | 5 |
| Static method | 3 | 4 |
| Constructor / default constructor | 4 | 5 |

The reasoning:

- The script initializer starts in the global scope, pushes a local scope, and
  for each class pushes the class scope while running `newclass`.
- Static/class code sees global, Object and class scopes (depth 3) and then its
  own local scope (depth 4).
- Instance code additionally sees the instance scope (depth 4), then its local
  scope (depth 5).

### Script initializer

```text
getlocal0
pushscope
for each class:
    getscopeobject 0          // global object, kept as initproperty target
    getlex <base QName>       // basetype operand for newclass
    dup
    pushscope                 // base class object becomes a scope
    newclass <class index>    // pops basetype, pushes the new class
    popscope
    initproperty <class QName>
returnvoid
```

`newclass` pops the basetype value from the operand stack and requires the
base class object on the scope stack (the scope stack is captured by the new
`ClassClosure`). Pushing the class's own multiname instead produces
`TypeError #1009` inside `global$init` at runtime — verified against Ruffle.

The `ScriptInfo` carries one `Class` trait per class and one `Function` trait per
package function. Executing the script initializer installs those traits on the
global object.

### Class initializer

```text
getlocal0
pushscope
findproperty <static field QName>
<push initializer or default value>
<coerce to field type>
setproperty <static field QName>       (initproperty for const)
returnvoid
```

Static fields with literal `const` values are stored in the trait itself; only
non-literal initializers need executable code.

### Constructor

```text
getlocal0
pushscope
getlocal0
constructsuper 0
<instance field initializers>
<constructor body>
returnvoid
```

Instance field initializers run after the base constructor, matching
ActionScript semantics. Literal `const` fields are covered by their trait value.
The constructor body is emitted in `InstanceInfo.init_method`.

## Bytecode shape

The emitter has a small stack tracker and a label/fixup mechanism. Conditional
jumps use `iftrue`, `iffalse`, `ifeq`, `ifne`, `iflt`, `ifle`, `ifgt`, `ifge`,
`ifstricteq` and `ifstrictne`; unconditional jumps use `jump`. This avoids
emitting a separate boolean materialization for every condition.

Notable operations:

- locals use short `getlocal0..3` / `setlocal0..3` forms where possible;
- property access uses `getlex`, `getproperty`, `setproperty`, `initproperty`,
  `findproperty`, `findpropstrict` and `getscopeobject`;
- free/global functions (`trace`, …) are called as
  `findpropstrict <name>` → args → `callproperty <name>`, because
  `callproperty` resolves the receiver's own traits and the builtins live on
  the playerglobal object reached through the scope chain;
- `pushdouble` stores the value in the double constant pool and emits a u30
  pool index (never inline bytes);
- calls use `callproperty`, `call`, `construct` and `constructsuper`;
- coercion uses `coerce` with the target multiname; numeric conversions use
  `convert_i`, `convert_u`, `convert_d`, `convert_b`, `convert_s`, `convert_o`;
- arithmetic, bitwise, comparison, strict equality, `instanceof`, `is` and `as`
  map directly to their AVM2 opcodes;
- `this` is local slot 0;
- method and script bodies end with `returnvoid` or `returnvalue` on every path.

`max_stack` is tracked conservatively and clamped to at least 64, which covers
the temporary objects used by initializers and nested calls. It is never
under-reported.

## Explicit omissions

The serialized ABC is complete for the supported subset, but the following AVM2
features are not generated:

- **Metadata:** the metadata array is serialized with count 0. Source metadata
  annotations such as `[Event(...)]` are not parsed or preserved.
- **Exception handlers:** every method body writes an exception count of 0.
- **Interfaces:** `InstanceInfo.interfaces` is always empty because `implements`
  is rejected.
- **Advanced multinames:** only QName and NamespaceSet are emitted. RTQName,
  MultinameL, explicit-namespace and generic multinames are not needed by the
  subset.
- **ABC flags beyond QName defaults:** `MethodInfo.flags` covers argument
  presence/optional arguments, not native/setter/getter/abstract methods.
- **Debug information:** no line-number or local-name tables are emitted. Source
  locations exist in compiler diagnostics, not in the ABC.

These omissions are documented rather than silently assumed. A player does not
need them to execute the emitted subset, but a debugger will not show source
lines and reflection will not see custom metadata.

## Validation

The test suite parses emitted ABC with Ruffle's independent reader:

```rust
let abc = swf::avm2::read::Reader::new(&unit.abc).read()?;
```

Integration tests then inspect instance/class counts, QNames, method bodies,
scope depths, constructor bytecode shape, field traits and the script class
traits. `nova-build` tests additionally parse the ABC payload out of a complete
SWF's `DoABC2` tag.

This proves the byte stream is structurally valid ABC and that it survives SWF
embedding. It does **not** prove verifier acceptance or runtime behavior in a
Flash Player Projector or Ruffle. The next validation step is a real runtime
smoke test; see [ROADMAP.md](ROADMAP.md).
