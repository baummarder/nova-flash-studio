//! AVM2 ABC data structures and serializer.
//!
//! Index zero of every constant pool is reserved (the AVM2 specification
//! reserves it, and Adobe's compiler uses it as "none"/`Any`). Entries are
//! stored at their real ABC index; index zero is never serialized.

use std::collections::HashMap;

use super::opcode::{write_i32, write_u30, write_u32};

pub const ABC_MINOR_VERSION: u16 = 16;
pub const ABC_MAJOR_VERSION: u16 = 46;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NsKind {
    /// Public namespace of a package (0x16).
    Package,
    /// Private namespace (0x05).
    Private,
    /// Protected namespace (0x18).
    Protected,
    /// Internal namespace of a package (0x17).
    PackageInternal,
}

impl NsKind {
    fn byte(self) -> u8 {
        match self {
            Self::Package => 0x16,
            Self::Private => 0x05,
            Self::Protected => 0x18,
            Self::PackageInternal => 0x17,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Namespace {
    pub kind: NsKind,
    pub name: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Multiname {
    QName {
        namespace: u32,
        name: u32,
    },
    /// `Multiname` (0x09) with a namespace set, used for dynamic lookups on
    /// external/inherited members.
    NamespaceSet {
        namespace_set: u32,
        name: u32,
    },
    /// `MultinameL` (0x1B): the property name is taken from the operand
    /// stack. Used by `receiver[index]` reads and writes.
    Late,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Undefined,
    Null,
    True,
    False,
    Int(u32),
    Uint(u32),
    Double(u32),
    String(u32),
}

/// Constant pool with reserved index zero.
#[derive(Debug, Clone)]
pub struct ConstantPool {
    pub ints: Vec<i32>,
    pub uints: Vec<u32>,
    pub doubles: Vec<f64>,
    pub strings: Vec<String>,
    pub namespaces: Vec<Namespace>,
    pub namespace_sets: Vec<Vec<u32>>,
    pub multinames: Vec<Multiname>,
    int_cache: HashMap<i32, u32>,
    uint_cache: HashMap<u32, u32>,
    double_cache: HashMap<u64, u32>,
    string_cache: HashMap<String, u32>,
    namespace_cache: HashMap<(NsKind, u32), u32>,
    multiname_cache: HashMap<(u32, u32), u32>,
    namespace_set_cache: HashMap<Vec<u32>, u32>,
    multiname_set_cache: HashMap<(u32, u32), u32>,
    multiname_l: Option<u32>,
}

impl Default for ConstantPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstantPool {
    pub fn new() -> Self {
        let mut string_cache = HashMap::new();
        // Index 1 is the empty string, matching Adobe's pools.
        string_cache.insert(String::new(), 1);
        Self {
            ints: vec![0],
            uints: vec![0],
            doubles: vec![0.0],
            strings: vec![String::new(), String::new()],
            namespaces: vec![Namespace {
                kind: NsKind::Package,
                name: 0,
            }],
            namespace_sets: vec![Vec::new()],
            multinames: vec![Multiname::QName {
                namespace: 0,
                name: 0,
            }],
            int_cache: HashMap::new(),
            uint_cache: HashMap::new(),
            double_cache: HashMap::new(),
            string_cache,
            namespace_cache: HashMap::new(),
            multiname_cache: HashMap::new(),
            namespace_set_cache: HashMap::new(),
            multiname_set_cache: HashMap::new(),
            multiname_l: None,
        }
    }

    pub fn intern_string(&mut self, value: &str) -> u32 {
        if let Some(index) = self.string_cache.get(value) {
            return *index;
        }
        let index = self.strings.len() as u32;
        self.strings.push(value.to_string());
        self.string_cache.insert(value.to_string(), index);
        index
    }

    pub fn add_int(&mut self, value: i32) -> u32 {
        if let Some(index) = self.int_cache.get(&value) {
            return *index;
        }
        let index = self.ints.len() as u32;
        self.ints.push(value);
        self.int_cache.insert(value, index);
        index
    }

    pub fn add_uint(&mut self, value: u32) -> u32 {
        if let Some(index) = self.uint_cache.get(&value) {
            return *index;
        }
        let index = self.uints.len() as u32;
        self.uints.push(value);
        self.uint_cache.insert(value, index);
        index
    }

    pub fn add_double(&mut self, value: f64) -> u32 {
        let key = value.to_bits();
        if let Some(index) = self.double_cache.get(&key) {
            return *index;
        }
        let index = self.doubles.len() as u32;
        self.doubles.push(value);
        self.double_cache.insert(key, index);
        index
    }

    pub fn add_namespace(&mut self, kind: NsKind, name: u32) -> u32 {
        let key = (kind, name);
        if let Some(index) = self.namespace_cache.get(&key) {
            return *index;
        }
        let index = self.namespaces.len() as u32;
        self.namespaces.push(Namespace { kind, name });
        self.namespace_cache.insert(key, index);
        index
    }

    pub fn add_qname(&mut self, namespace: u32, name: u32) -> u32 {
        let key = (namespace, name);
        if let Some(index) = self.multiname_cache.get(&key) {
            return *index;
        }
        let index = self.multinames.len() as u32;
        self.multinames.push(Multiname::QName { namespace, name });
        self.multiname_cache.insert(key, index);
        index
    }

    /// Interns a namespace set. The empty set is reserved at index zero.
    pub fn add_namespace_set(&mut self, mut namespaces: Vec<u32>) -> u32 {
        namespaces.sort_unstable();
        namespaces.dedup();
        if let Some(index) = self.namespace_set_cache.get(&namespaces) {
            return *index;
        }
        let index = self.namespace_sets.len() as u32;
        self.namespace_sets.push(namespaces.clone());
        self.namespace_set_cache.insert(namespaces, index);
        index
    }

    /// Interns a multiname that resolves the name through a namespace set.
    pub fn add_multiname(&mut self, namespace_set: u32, name: u32) -> u32 {
        let key = (namespace_set, name);
        if let Some(index) = self.multiname_set_cache.get(&key) {
            return *index;
        }
        let index = self.multinames.len() as u32;
        self.multinames.push(Multiname::NamespaceSet {
            namespace_set,
            name,
        });
        self.multiname_set_cache.insert(key, index);
        index
    }

    /// Interns the single `MultinameL` entry used for dynamic index access.
    pub fn add_multiname_l(&mut self) -> u32 {
        if let Some(index) = self.multiname_l {
            return index;
        }
        let index = self.multinames.len() as u32;
        self.multinames.push(Multiname::Late);
        self.multiname_l = Some(index);
        index
    }
}

#[derive(Debug, Clone)]
pub struct MethodParam {
    pub ty: u32,
    pub name: u32,
    pub default: Option<DefaultValue>,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: u32,
    pub return_type: u32,
    pub params: Vec<MethodParam>,
    pub flags: u8,
}

#[derive(Debug, Clone)]
pub enum TraitKind {
    Slot {
        slot_id: u32,
        type_name: u32,
        value: Option<DefaultValue>,
    },
    Method {
        disp_id: u32,
        method: u32,
    },
    Class {
        slot_id: u32,
        class: u32,
    },
    Function {
        slot_id: u32,
        function: u32,
    },
    Const {
        slot_id: u32,
        type_name: u32,
        value: Option<DefaultValue>,
    },
}

#[derive(Debug, Clone)]
pub struct Trait {
    pub name: u32,
    pub kind: TraitKind,
    pub is_final: bool,
    pub is_override: bool,
}

#[derive(Debug, Clone)]
pub struct InstanceInfo {
    pub name: u32,
    pub super_name: u32,
    pub is_sealed: bool,
    pub is_final: bool,
    pub protected_namespace: Option<u32>,
    pub interfaces: Vec<u32>,
    pub init_method: u32,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub init_method: u32,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone)]
pub struct ScriptInfo {
    pub init_method: u32,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone)]
pub struct MethodBody {
    pub method: u32,
    pub max_stack: u32,
    pub num_locals: u32,
    pub init_scope_depth: u32,
    pub max_scope_depth: u32,
    pub code: Vec<u8>,
    pub traits: Vec<Trait>,
}

#[derive(Debug, Clone, Default)]
pub struct AbcFile {
    pub constant_pool: ConstantPool,
    pub methods: Vec<MethodInfo>,
    pub instances: Vec<InstanceInfo>,
    pub classes: Vec<ClassInfo>,
    pub scripts: Vec<ScriptInfo>,
    pub method_bodies: Vec<MethodBody>,
}

impl AbcFile {
    pub fn new() -> Self {
        Self::default()
    }

    /// Serializes the file in the AVM2 ABC layout.
    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&ABC_MINOR_VERSION.to_le_bytes());
        out.extend_from_slice(&ABC_MAJOR_VERSION.to_le_bytes());
        self.write_constant_pool(&mut out);

        write_u30(&mut out, self.methods.len() as u32);
        for method in &self.methods {
            write_method(&mut out, method);
        }

        // Metadata is unsupported by the subset compiler; its count is zero.
        write_u30(&mut out, 0);

        write_u30(&mut out, self.instances.len() as u32);
        for instance in &self.instances {
            write_instance(&mut out, instance);
        }

        // Note: the class section has no count. There is exactly one ClassInfo
        // per InstanceInfo, in the same order.
        for class in &self.classes {
            write_class(&mut out, class);
        }

        write_u30(&mut out, self.scripts.len() as u32);
        for script in &self.scripts {
            write_script(&mut out, script);
        }

        write_u30(&mut out, self.method_bodies.len() as u32);
        for body in &self.method_bodies {
            write_method_body(&mut out, body);
        }

        out
    }

    fn write_constant_pool(&self, out: &mut Vec<u8>) {
        // Count includes the reserved index zero.
        write_u30(out, self.constant_pool.ints.len() as u32);
        for value in self.constant_pool.ints.iter().skip(1) {
            write_i32(out, *value);
        }

        write_u30(out, self.constant_pool.uints.len() as u32);
        for value in self.constant_pool.uints.iter().skip(1) {
            write_u32(out, *value);
        }

        write_u30(out, self.constant_pool.doubles.len() as u32);
        for value in self.constant_pool.doubles.iter().skip(1) {
            out.extend_from_slice(&value.to_le_bytes());
        }

        write_u30(out, self.constant_pool.strings.len() as u32);
        for value in self.constant_pool.strings.iter().skip(1) {
            let bytes = value.as_bytes();
            write_u30(out, bytes.len() as u32);
            out.extend_from_slice(bytes);
        }

        write_u30(out, self.constant_pool.namespaces.len() as u32);
        for namespace in self.constant_pool.namespaces.iter().skip(1) {
            out.push(namespace.kind.byte());
            write_u30(out, namespace.name);
        }

        write_u30(out, self.constant_pool.namespace_sets.len() as u32);
        for set in self.constant_pool.namespace_sets.iter().skip(1) {
            write_u30(out, set.len() as u32);
            for index in set {
                write_u30(out, *index);
            }
        }

        write_u30(out, self.constant_pool.multinames.len() as u32);
        for multiname in self.constant_pool.multinames.iter().skip(1) {
            match multiname {
                Multiname::QName { namespace, name } => {
                    out.push(0x07);
                    write_u30(out, *namespace);
                    write_u30(out, *name);
                }
                Multiname::NamespaceSet {
                    namespace_set,
                    name,
                } => {
                    out.push(0x09);
                    // The AVM2 layout stores the local name before the
                    // namespace-set index for kind 0x09.
                    write_u30(out, *name);
                    write_u30(out, *namespace_set);
                }
                Multiname::Late => {
                    out.push(0x1b);
                    // MultinameL carries a namespace-set operand. The empty
                    // set (index zero) selects the public/dynamic namespace.
                    write_u30(out, 0);
                }
            }
        }
    }
}

fn write_method(out: &mut Vec<u8>, method: &MethodInfo) {
    write_u30(out, method.params.len() as u32);
    write_u30(out, method.return_type);
    for param in &method.params {
        write_u30(out, param.ty);
    }
    write_u30(out, method.name);
    out.push(method.flags);

    let optional: Vec<&MethodParam> = method
        .params
        .iter()
        .filter(|param| param.default.is_some())
        .collect();
    if !optional.is_empty() {
        write_u30(out, optional.len() as u32);
        for param in optional {
            if let Some(value) = param.default {
                write_constant_value(out, value);
            }
        }
    }

    if method.flags & 0x80 != 0 {
        for param in &method.params {
            write_u30(out, param.name);
        }
    }
}

fn write_constant_value(out: &mut Vec<u8>, value: DefaultValue) {
    let (index, kind) = match value {
        DefaultValue::Undefined => (0, 0x00),
        DefaultValue::String(index) => (index, 0x01),
        DefaultValue::Int(index) => (index, 0x03),
        DefaultValue::Uint(index) => (index, 0x04),
        DefaultValue::Double(index) => (index, 0x06),
        DefaultValue::False => (0x0a, 0x0a),
        DefaultValue::True => (0x0b, 0x0b),
        DefaultValue::Null => (0x0c, 0x0c),
    };
    write_u30(out, index);
    out.push(kind);
}

fn write_optional_value(out: &mut Vec<u8>, value: Option<DefaultValue>) {
    match value {
        None => write_u30(out, 0),
        Some(value) => write_constant_value(out, value),
    }
}

fn write_trait(out: &mut Vec<u8>, trait_info: &Trait) {
    write_u30(out, trait_info.name);
    let mut flags = 0u8;
    if trait_info.is_final {
        flags |= 0x10;
    }
    if trait_info.is_override {
        flags |= 0x20;
    }
    match trait_info.kind {
        TraitKind::Slot {
            slot_id,
            type_name,
            value,
        } => {
            out.push(flags);
            write_u30(out, slot_id);
            write_u30(out, type_name);
            write_optional_value(out, value);
        }
        TraitKind::Method { disp_id, method } => {
            out.push(flags | 1);
            write_u30(out, disp_id);
            write_u30(out, method);
        }
        TraitKind::Class { slot_id, class } => {
            out.push(flags | 4);
            write_u30(out, slot_id);
            write_u30(out, class);
        }
        TraitKind::Function { slot_id, function } => {
            out.push(flags | 5);
            write_u30(out, slot_id);
            write_u30(out, function);
        }
        TraitKind::Const {
            slot_id,
            type_name,
            value,
        } => {
            out.push(flags | 6);
            write_u30(out, slot_id);
            write_u30(out, type_name);
            write_optional_value(out, value);
        }
    }
}

fn write_instance(out: &mut Vec<u8>, instance: &InstanceInfo) {
    write_u30(out, instance.name);
    write_u30(out, instance.super_name);
    let mut flags = 0u8;
    if instance.is_sealed {
        flags |= 0x01;
    }
    if instance.is_final {
        flags |= 0x02;
    }
    if instance.protected_namespace.is_some() {
        flags |= 0x08;
    }
    out.push(flags);
    if let Some(namespace) = instance.protected_namespace {
        write_u30(out, namespace);
    }
    write_u30(out, instance.interfaces.len() as u32);
    for interface in &instance.interfaces {
        write_u30(out, *interface);
    }
    write_u30(out, instance.init_method);
    write_u30(out, instance.traits.len() as u32);
    for trait_info in &instance.traits {
        write_trait(out, trait_info);
    }
}

fn write_class(out: &mut Vec<u8>, class: &ClassInfo) {
    write_u30(out, class.init_method);
    write_u30(out, class.traits.len() as u32);
    for trait_info in &class.traits {
        write_trait(out, trait_info);
    }
}

fn write_script(out: &mut Vec<u8>, script: &ScriptInfo) {
    write_u30(out, script.init_method);
    write_u30(out, script.traits.len() as u32);
    for trait_info in &script.traits {
        write_trait(out, trait_info);
    }
}

fn write_method_body(out: &mut Vec<u8>, body: &MethodBody) {
    write_u30(out, body.method);
    write_u30(out, body.max_stack);
    write_u30(out, body.num_locals);
    write_u30(out, body.init_scope_depth);
    write_u30(out, body.max_scope_depth);
    write_u30(out, body.code.len() as u32);
    out.extend_from_slice(&body.code);
    write_u30(out, 0); // exception count
    write_u30(out, body.traits.len() as u32);
    for trait_info in &body.traits {
        write_trait(out, trait_info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pools_reserve_index_zero() {
        let mut pool = ConstantPool::new();
        assert_eq!(pool.intern_string(""), 1);
        assert_eq!(pool.intern_string("Main"), 2);
        assert_eq!(pool.intern_string("Main"), 2);
        let namespace_name = pool.intern_string("flash.display");
        let ns = pool.add_namespace(NsKind::Package, namespace_name);
        let name = pool.intern_string("MovieClip");
        let qname = pool.add_qname(ns, name);
        assert!(qname >= 1);
        assert_eq!(pool.add_qname(ns, name), qname);
    }

    #[test]
    fn empty_abc_serializes_header() {
        let abc = AbcFile::new();
        let bytes = abc.write();
        assert_eq!(&bytes[..4], &[0x10, 0x00, 0x2e, 0x00]);
    }
}
