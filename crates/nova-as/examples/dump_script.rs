//! Debug helper: compile an .as file and print ABC structure + method bodies.
use std::path::Path;
use swf::avm2::types::{Index, Multiname, Namespace};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: dump_script <file.as>");
    let source = std::fs::read_to_string(Path::new(&path)).expect("read source");
    let unit = nova_as::compile(&source);
    for d in &unit.diagnostics {
        eprintln!("{}", d.render(&source, &path));
    }
    if unit.has_errors() {
        std::process::exit(1);
    }
    let abc = swf::avm2::read::Reader::new(&unit.abc)
        .read()
        .expect("abc parse");
    let cp = &abc.constant_pool;

    let s = |i: Index<String>| {
        String::from_utf8_lossy(
            cp.strings
                .get(i.0 as usize - 1)
                .cloned()
                .unwrap_or_default()
                .as_slice(),
        )
        .into_owned()
    };
    let ns_name = |i: Index<Namespace>| -> String {
        match cp.namespaces.get(i.0 as usize - 1) {
            Some(Namespace::Package(n)) | Some(Namespace::PackageInternal(n)) => s(*n),
            Some(Namespace::Namespace(n)) => s(*n),
            Some(Namespace::Private(n)) => format!("private:{}", s(*n)),
            Some(Namespace::Protected(n)) => format!("protected:{}", s(*n)),
            _ => "?".into(),
        }
    };
    for (i, m) in cp.multinames.iter().enumerate() {
        let desc = match m {
            Multiname::QName { namespace, name } => {
                format!("QName({}, {})", ns_name(*namespace), s(*name))
            }
            Multiname::Multiname {
                namespace_set,
                name,
            } => {
                format!("Multiname(nsset {} , {})", namespace_set.0, s(*name))
            }
            Multiname::MultinameL { .. } => "MultinameL".into(),
            Multiname::TypeName { .. } => "TypeName".into(),
            _ => format!("{:?}", m),
        };
        println!("m[{}] = {}", i + 1, desc);
    }
    for (i, body) in abc.method_bodies.iter().enumerate() {
        println!(
            "body[{i}] init_scope={} max_scope={} locals={} code:",
            body.init_scope_depth, body.max_scope_depth, body.num_locals
        );
        for chunk in body.code.chunks(24) {
            println!(
                "  {}",
                chunk
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }
}
