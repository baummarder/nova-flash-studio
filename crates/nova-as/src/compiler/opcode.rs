//! AVM2 opcode constants and operand encoders.
//!
//! Only the opcodes used by the subset compiler are declared. Byte values match
//! the AVM2 specification (and `swf::avm2::opcode::OpCode`).

// Control flow.
pub const NOP: u8 = 0x02;
pub const THROW: u8 = 0x03;
pub const KILL: u8 = 0x08;
pub const LABEL: u8 = 0x09;
pub const JUMP: u8 = 0x10;
pub const IFTRUE: u8 = 0x11;
pub const IFFALSE: u8 = 0x12;
pub const IFEQ: u8 = 0x13;
pub const IFNE: u8 = 0x14;
pub const IFLT: u8 = 0x15;
pub const IFLE: u8 = 0x16;
pub const IFGT: u8 = 0x17;
pub const IFGE: u8 = 0x18;
pub const IFSTRICTEQ: u8 = 0x19;
pub const IFSTRICTNE: u8 = 0x1a;
pub const PUSHWITH: u8 = 0x1c;
pub const POPSCOPE: u8 = 0x1d;
pub const PUSHNULL: u8 = 0x20;
pub const PUSHUNDEFINED: u8 = 0x21;
pub const PUSHBYTE: u8 = 0x24;
pub const PUSHSHORT: u8 = 0x25;
pub const PUSHTRUE: u8 = 0x26;
pub const PUSHFALSE: u8 = 0x27;
pub const PUSHNAN: u8 = 0x28;
pub const POP: u8 = 0x29;
pub const DUP: u8 = 0x2a;
pub const SWAP: u8 = 0x2b;
pub const PUSHSTRING: u8 = 0x2c;
pub const PUSHINT: u8 = 0x2d;
pub const PUSHUINT: u8 = 0x2e;
pub const PUSHDOUBLE: u8 = 0x2f;
pub const PUSHSCOPE: u8 = 0x30;
pub const RETURNVOID: u8 = 0x47;
pub const RETURNVALUE: u8 = 0x48;

// Object / property access.
pub const CONSTRUCTSUPER: u8 = 0x49;
pub const CONSTRUCTPROP: u8 = 0x4a;
pub const CONSTRUCT: u8 = 0x42;
pub const CALLPROPERTY: u8 = 0x46;
pub const CALLPROPVOID: u8 = 0x4f;
pub const CALL: u8 = 0x41;
pub const GETLEX: u8 = 0x60;
pub const GETPROPERTY: u8 = 0x66;
pub const SETPROPERTY: u8 = 0x61;
pub const DELETEPROPERTY: u8 = 0x6a;
pub const INITPROPERTY: u8 = 0x68;
pub const GETSCOPEOBJECT: u8 = 0x65;
pub const GETGLOBALSCOPE: u8 = 0x64;
pub const GETLOCAL: u8 = 0x62;
pub const GETLOCAL0: u8 = 0xd0;
pub const GETLOCAL1: u8 = 0xd1;
pub const GETLOCAL2: u8 = 0xd2;
pub const GETLOCAL3: u8 = 0xd3;
pub const SETLOCAL: u8 = 0x63;
pub const SETLOCAL0: u8 = 0xd4;
pub const SETLOCAL1: u8 = 0xd5;
pub const SETLOCAL2: u8 = 0xd6;
pub const SETLOCAL3: u8 = 0xd7;
pub const NEWCLASS: u8 = 0x58;
pub const NEWFUNCTION: u8 = 0x40;
pub const NEWARRAY: u8 = 0x56;
pub const NEWOBJECT: u8 = 0x55;
pub const FINDPROPERTY: u8 = 0x5e;
pub const FINDPROPSTRICT: u8 = 0x5d;

// Coercions.
pub const COERCE: u8 = 0x80;
pub const COERCE_B: u8 = 0x81;
pub const COERCE_A: u8 = 0x82;
pub const COERCE_I: u8 = 0x83;
pub const COERCE_D: u8 = 0x84;
pub const COERCE_S: u8 = 0x85;
pub const COERCE_U: u8 = 0x88;
pub const COERCE_O: u8 = 0x89;
pub const CONVERT_S: u8 = 0x70;
pub const CONVERT_I: u8 = 0x73;
pub const CONVERT_U: u8 = 0x74;
pub const CONVERT_D: u8 = 0x75;
pub const CONVERT_B: u8 = 0x76;
pub const CONVERT_O: u8 = 0x77;

// Operators.
pub const NEGATE: u8 = 0x90;
pub const INCREMENT: u8 = 0x91;
pub const DECREMENT: u8 = 0x93;
pub const NOT: u8 = 0x96;
pub const BITNOT: u8 = 0x97;
pub const TYPEOF: u8 = 0x95;
pub const ADD: u8 = 0xa0;
pub const SUBTRACT: u8 = 0xa1;
pub const MULTIPLY: u8 = 0xa2;
pub const DIVIDE: u8 = 0xa3;
pub const MODULO: u8 = 0xa4;
pub const LSHIFT: u8 = 0xa5;
pub const RSHIFT: u8 = 0xa6;
pub const URSHIFT: u8 = 0xa7;
pub const BITAND: u8 = 0xa8;
pub const BITOR: u8 = 0xa9;
pub const BITXOR: u8 = 0xaa;
pub const EQUALS: u8 = 0xab;
pub const STRICTEQUALS: u8 = 0xac;
pub const LESSTHAN: u8 = 0xad;
pub const LESSEQUALS: u8 = 0xae;
pub const GREATERTHAN: u8 = 0xaf;
pub const GREATEREQUALS: u8 = 0xb0;
pub const INSTANCEOF: u8 = 0xb1;
pub const ISTYPE: u8 = 0xb2;
pub const ASTYPE: u8 = 0x86;

/// Appends an AVM2 30-bit variable length integer.
pub fn write_u30(out: &mut Vec<u8>, value: u32) {
    let mut value = value & 0x3fff_ffff;
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            out.push(byte | 0x80);
        } else {
            out.push(byte);
            break;
        }
    }
}

/// Appends a full 32-bit unsigned value using the AVM2 variable length
/// representation (up to five bytes). Unlike [`write_u30`] this keeps the top
/// two bits, which is required for `u32` constants and operands.
pub fn write_u32(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            out.push(byte | 0x80);
        } else {
            out.push(byte);
            break;
        }
    }
}

/// Appends an `i32` using the AVM2 encoded-s32 representation. Negative values
/// sign-extend through the fifth byte (`0x0f`/`0x1f`), not through a masked
/// 30-bit value.
pub fn write_i32(out: &mut Vec<u8>, value: i32) {
    write_u32(out, value as u32);
}

pub fn op(opcode: u8) -> Vec<u8> {
    vec![opcode]
}

pub fn op_u8(opcode: u8, value: u8) -> Vec<u8> {
    vec![opcode, value]
}

pub fn op_u30(opcode: u8, value: u32) -> Vec<u8> {
    let mut out = vec![opcode];
    write_u30(&mut out, value);
    out
}

pub fn op_u30_u30(opcode: u8, first: u32, second: u32) -> Vec<u8> {
    let mut out = vec![opcode];
    write_u30(&mut out, first);
    write_u30(&mut out, second);
    out
}

/// `getlocal <slot>` using the short forms for slots 0..=3.
pub fn get_local(slot: u32) -> Vec<u8> {
    match slot {
        0 => op(GETLOCAL0),
        1 => op(GETLOCAL1),
        2 => op(GETLOCAL2),
        3 => op(GETLOCAL3),
        _ => op_u30(GETLOCAL, slot),
    }
}

/// `setlocal <slot>` using the short forms for slots 0..=3.
pub fn set_local(slot: u32) -> Vec<u8> {
    match slot {
        0 => op(SETLOCAL0),
        1 => op(SETLOCAL1),
        2 => op(SETLOCAL2),
        3 => op(SETLOCAL3),
        _ => op_u30(SETLOCAL, slot),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_u30_boundaries() {
        let mut out = Vec::new();
        write_u30(&mut out, 0);
        write_u30(&mut out, 0x7f);
        write_u30(&mut out, 0x80);
        write_u30(&mut out, 0x3fff_ffff);
        assert_eq!(
            out,
            vec![0x00, 0x7f, 0x80, 0x01, 0xff, 0xff, 0xff, 0xff, 0x03]
        );
    }

    #[test]
    fn negative_i32_uses_five_sign_extended_bytes() {
        let mut out = Vec::new();
        write_i32(&mut out, -64);
        assert_eq!(out, vec![0xc0, 0xff, 0xff, 0xff, 0x0f]);
        let mut out = Vec::new();
        write_i32(&mut out, -1);
        assert_eq!(out, vec![0xff, 0xff, 0xff, 0xff, 0x0f]);
    }

    #[test]
    fn full_u32_keeps_high_bits() {
        let mut out = Vec::new();
        write_u32(&mut out, u32::MAX);
        assert_eq!(out, vec![0xff, 0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn short_local_forms() {
        assert_eq!(get_local(0), vec![0xd0]);
        assert_eq!(get_local(4), vec![0x62, 4]);
        assert_eq!(set_local(3), vec![0xd7]);
    }
}
