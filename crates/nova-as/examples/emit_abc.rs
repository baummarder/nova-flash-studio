fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: emit_abc <file.as> [out.abc]");
    let source = std::fs::read_to_string(&path).unwrap();
    let unit = nova_as::compile(&source);
    for d in &unit.diagnostics {
        eprintln!("{}", d.render(&source, &path));
    }
    if unit.has_errors() {
        std::process::exit(1);
    }
    let bytes = &unit.abc;
    if let Some(out) = std::env::args().nth(2) {
        std::fs::write(&out, bytes).unwrap();
        eprintln!("wrote {} bytes to {out}", bytes.len());
    } else {
        use std::io::Write;
        std::io::stdout().write_all(bytes).unwrap();
    }
}
