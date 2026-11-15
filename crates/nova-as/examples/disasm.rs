fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: disasm <file.swf|file.abc>");
    let data = std::fs::read(&path).unwrap();
    // If raw ABC (starts with minor/major 0x10 0x00), use directly; else find DoABC2 in SWF.
    let abcs: Vec<Vec<u8>> = if data.len() > 4 && data[0] == 0x10 && data[1] == 0x00 {
        vec![data]
    } else {
        extract_doabc(&data)
    };
    for (i, abc) in abcs.iter().enumerate() {
        println!("=== ABC {i} ({} bytes) ===", abc.len());
        match swf::avm2::read::Reader::new(&abc[..]).read() {
            Ok(file) => dump_abc(&file),
            Err(e) => println!("parse error: {e:?}"),
        }
    }
}

fn extract_doabc(data: &[u8]) -> Vec<Vec<u8>> {
    // Minimal SWF tag walker (CWS/ZWS decompression via flate2 not available -> handle FWS only or skip)
    let out = Vec::new();
    if data.len() < 8 || &data[..3] != b"FWS" {
        // try CWS: header 8 bytes, rest zlib
        return out;
    }
    extract_doabc_fws(data)
}

fn extract_doabc_fws(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 8usize;
    while pos + 2 <= data.len() {
        let tag_head = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        let code = tag_head >> 6;
        let len = if tag_head & 0x3f == 0x3f {
            let l = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            pos += 4;
            l
        } else {
            (tag_head & 0x3f) as usize
        };
        if pos + len > data.len() {
            break;
        }
        if code == 82 {
            // DoABC2: u32 flags, cstring name, then abc bytes
            let body = &data[pos..pos + len];
            let name_end = body.iter().position(|&b| b == 0).unwrap_or(body.len());
            let abc_start = 4 + name_end + 1;
            out.push(body[abc_start..].to_vec());
        }
        pos += len;
    }
    out
}

fn dump_abc(file: &swf::avm2::types::AbcFile) {
    for (mi, m) in file.methods.iter().enumerate() {
        println!(
            "method[{mi}] params={} ret={:?}",
            m.params.len(),
            m.return_type
        );
    }
    for (bi, body) in file.method_bodies.iter().enumerate() {
        println!(
            "--- body[{bi}] method={:?} locals={} max_stack={} init_scope={} max_scope={} code_len={}",
            body.method.0,
            body.num_locals,
            body.max_stack,
            body.init_scope_depth,
            body.max_scope_depth,
            body.code.len()
        );
        // Re-serialize ops? swf crate stores ops; print raw code hex instead.
        for chunk in body.code.chunks(16) {
            print!("  ");
            for b in chunk {
                print!("{b:02x} ");
            }
            println!();
        }
    }
}
