# ActionScript 3 Compatibility Matrix

Status of the `nova-as` compiler as of `v0.3.0-rc1`. This document describes
the **implemented subset**, not full AS3. Every unsupported construct produces
a source-located diagnostic and aborts compilation — nothing is silently
dropped or approximated, and a failed compile never embeds partial ABC into a
SWF.

Verification levels used below:

- **Runtime-verified**: executed in Ruffle 0.7.1 (see
  `docs/SWF_RUNTIME_COMPATIBILITY.md`).
- **Structurally verified**: emitted ABC parses with the `swf` crate's AVM2
  reader and is covered by unit/integration tests, but has not been executed
  in a player.
- **Compile-only**: accepted by the checker/emitter; runtime behavior unproven.

## Packages, imports, classes

| Feature | Status | Notes |
| --- | --- | --- |
| `package { }` / named packages | Supported | Runtime-verified for the default package; named packages structurally verified. |
| `import a.b.C` | Supported | Maps simple names to qualified class names; used for `flash.display.*`, `flash.events.*`. Runtime-verified. |
| `import a.b.*` | Supported | Wildcard imports resolved in order. |
| `public` / `private` / `protected` / `internal` | Partial | Emitted as the matching AVM2 namespaces. Access *enforcement* beyond namespace encoding is limited; `private` members are emitted in the class's private namespace. |
| `final class` | Supported | Checker rejects `extends` of final classes. |
| `class X extends Y` | Supported | Single inheritance; base resolved through imports and local classes. `flash.display.MovieClip` chain runtime-verified. |
| `implements` / interfaces | Unsupported | Diagnostic; `InstanceInfo.interfaces` always empty. |
| Custom namespaces (`namespace`, `use namespace`) | Unsupported | Diagnostic. |
| `dynamic` classes | Unsupported | Rejected. |
| Default constructor synthesis | Supported | Runtime-verified. |
| Explicit `super(...)` args / `super.method()` | Unsupported / limited | Constructor calls `constructsuper 0` only; base constructors with required parameters are rejected by the checker. `super.method()` calls are not supported. |

## Members and types

| Feature | Status | Notes |
| --- | --- | --- |
| Instance fields (`var`, typed, initializers) | Supported | Runtime-verified incl. `Number`/`int` defaults and coercion. |
| `const` fields | Partial | Literal consts stored in trait values; non-literal initializers run in `cinit`. |
| Static fields / static methods | Supported | Structurally verified; `cinit` uses `findproperty`/`setproperty`. |
| Instance methods | Supported | Runtime-verified (event listener callback). |
| Private methods referenced as values (`this.update`) | Runtime-verified | `getproperty` on the private-namespace multiname returns a callable closure. |
| Getters / setters | Unsupported | Diagnostic at parse/check time. |
| `*` (untyped) parameters/returns | Supported | Mapped to multiname index 0 (`*`). |
| Primitive types `int uint Number Boolean String Object void` | Supported | Coercion opcodes emitted. |
| Class-typed references | Supported | `Type::Class` multinames; external Flash classes treated as opaque. |
| `Array`, `Vector`, `Function` types | Partial | `Array`/`Function` resolve to `None` in `resolve_type_name`; array/vector literals and APIs are unsupported. |

## Statements

| Feature | Status | Notes |
| --- | --- | --- |
| Blocks, `var` declarations | Supported | Runtime-verified. |
| `if` / `else` | Supported | Runtime-verified. |
| `while`, `do..while` | Supported | Structurally verified. |
| `for` | Supported | Classic `for(init;cond;step)`; `for-in`/`for-each` unsupported. |
| `break` / `continue` | Supported | Structurally verified. |
| `return` (value and void) | Supported | Runtime-verified. |
| Expression statements | Supported | Runtime-verified. |
| `throw`, `try`/`catch`/`finally` | Unsupported | `throw` parses but is rejected; exceptions tables are always empty. |
| `switch` | Unsupported | Diagnostic. |
| `for-in` / `for-each` | Unsupported | Diagnostic. |

## Expressions and operators

| Feature | Status | Notes |
| --- | --- | --- |
| Numeric/string/bool/null literals, `this` | Supported | Runtime-verified. `NaN`, `Infinity` supported. |
| Arithmetic `+ - * / %`, unary `- ! ~`, prefix `++/--` | Supported | Runtime-verified for the sample paths. |
| Comparison `< <= > >= == != === !==` | Supported | Runtime-verified (`>`, `<` in `update`). |
| Bitwise `& | ^ << >> >>>` | Supported | Structurally verified. |
| Assignment `= += -= *= /= %= ...` | Supported | Runtime-verified incl. compound member assignment. |
| Ternary `?:` | Supported | Structurally verified. |
| Member access `a.b`, indexing `a[i]` | Partial | Static/dynamic member multinames emitted; `a[i]` runtime behavior unverified. |
| Calls `f(...)`, `new X(...)` | Supported | Runtime-verified for `trace`, `addEventListener`, constructors. |
| Method values (`this.update`, `obj.method`) | Runtime-verified | See private-method row above. |
| `is`, `as`, `instanceof` | Supported | Structurally verified; runtime check partial. |
| Closures / function expressions / nested functions | Unsupported | Only package-level functions are emitted as script traits. |
| `delete`, `in`, `typeof`, `void` | Unsupported / limited | Not part of the emitted subset. |
| Array/object literals | Unsupported | Diagnostic (parsed, rejected in check). |
| XML/E4X | Unsupported | — |

## Flash runtime API surface

The compiler does not model the Flash API. Names like `trace`,
`Event.ENTER_FRAME`, `addEventListener`, `x`, `rotation`, `gotoAndStop` resolve
as *dynamic* multinames or class references and are executed by the player.
Anything the player does not provide fails at runtime with a normal AVM2
error — the compiler cannot prove these calls correct.

| API used by samples | Status |
| --- | --- |
| `trace()` | Runtime-verified (Ruffle `avm_trace` output observed). |
| `flash.display.MovieClip` inheritance | Runtime-verified. |
| `flash.events.Event.ENTER_FRAME` + `addEventListener` | Runtime-verified (handlers ran every frame without errors). |
| `flash.display.Sprite`, `Stage`, `TextField`, etc. | Compile-only / untested. |
| Timer, Loader, NetStream, filters, text engine | Untested — likely unsupported in practice. |

## Imported FLA and SWF code

The compiler only applies to ActionScript source that is actually present:

| Import source | Stored as | Export behaviour |
| --- | --- | --- |
| `.nova` script source | `OriginalSource` | Compiled by `nova-as` |
| `.fla`/XFL `scripts/*.as` | `OriginalSource` | Compiled by `nova-as` |
| `.swf` `DoABC`/`DoABC2` | `CompiledBytecode` plus preserved ABC bytes | Embedded unchanged; not recompiled |
| Marker text for preserved ABC | Not source | Never presented as original or decompiled code |

There is no decompiler in this repository. A `.swf` import cannot reconstruct
original AS3 source, and the editor does not claim otherwise. `nova analyze`
reports provenance and byte counts so an agent or user can distinguish original,
reconstructed, generated and compiled content.

## Known runtime-verified bugs fixed during Milestone 6

These were real defects found by executing generated SWFs in Ruffle and are
now covered by tests or documented behavior:

1. Script initializer pushed the class's own (uninitialized) multiname as the
   `newclass` scope → `TypeError #1009` in `global$init`. Fixed: the basetype
   operand/scope sequence now matches the AVM2 spec.
2. Built-in calls used `getscopeobject 0`/`getglobalscope` as the
   `callproperty` receiver → `ReferenceError #1069` / `TypeError #1006`.
   Fixed: `findpropstrict` resolves the playerglobal owner as receiver.
3. Imported class names inside `MovieClip` subclasses were shadowed by the
   "inherited Flash member" fallback (`this.Event`). Fixed: class and package
   function resolution now precedes the dynamic fallback.
4. `pushdouble` emitted 8 inline bytes; AVM2 expects a u30 double-pool index →
   `VerifyError #1032`. Fixed: values are interned in the double pool.

## Honest limitations

- **Not full AS3.** Roughly half of the language surface is unsupported; see
  tables above. Unsupported code fails loudly at compile time.
- Access control is namespace-encoded, not fully enforced.
- External Flash classes are opaque: the checker cannot validate their
  members, so typos in API names surface only at runtime.
- Multi-class scripts require base classes to be declared before derived
  classes (declaration order is used for `newclass` sequencing).
- No debug info (line numbers, local names) is embedded in the ABC.
