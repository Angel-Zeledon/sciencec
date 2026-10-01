//! The bundled `encoding` module, **built, linked, run**.
//!
//! Expected values were computed outside the module, in Python (`base64`,
//! `bytes.hex`, `str.encode('utf-16-le')`), and are pinned here as text.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("encoding", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// Helpers every program shares: `dump` prints a decode as `ok HEX` or
/// `error MESSAGE`, `show` the same for text, `raw` builds bytes from hex.
const SHARED: &str = "use encoding

def raw(text: String) -> Array[U8]:
    let bytes, err be encoding.hex_decode(&text)
    if err?:
        panic(\"bad fixture\")
    bytes

def dump(bytes: Array[U8], err: encoding.EncodingError?):
    if err?:
        print(f\"error {err.message()}\")
    else:
        print(f\"ok {encoding.hex_encode(&bytes)}\")

def show(text: String, err: encoding.EncodingError?):
    if err?:
        print(f\"error {err.message()}\")
    else:
        print(f\"ok {text}\")
";

fn program(body: &str) -> String {
    format!("{SHARED}\ndef main():\n{body}")
}

#[test]
fn base64_matches_rfc_4648_vectors() {
    let out = prints(
        "b64_vectors",
        &program(
            "    for s in [\"\", \"f\", \"fo\", \"foo\", \"foob\", \"fooba\", \"foobar\"]:
        let bytes be encoding.utf8_encode(s)
        let text be encoding.base64_encode(&bytes)
        let back, err be encoding.base64_decode(&text)
        dump(back, err)
        print(f\"[{text}]\")
    let odd be raw(\"fbfffe000102\")
    print(encoding.base64_encode(&odd))
    print(encoding.base64_url_encode(&odd))
    print(encoding.base64_url_encode_padded(&odd))
    let one be raw(\"66\")
    print(encoding.base64_url_encode(&one))
    print(encoding.base64_url_encode_padded(&one))
",
        ),
    );
    assert_eq!(
        out,
        "ok \n[]\nok 66\n[Zg==]\nok 666f\n[Zm8=]\nok 666f6f\n[Zm9v]\nok 666f6f62\n[Zm9vYg==]\n\
         ok 666f6f6261\n[Zm9vYmE=]\nok 666f6f626172\n[Zm9vYmFy]\n\
         +//+AAEC\n-__-AAEC\n-__-AAEC\nZg\nZg==\n"
    );
}

#[test]
fn base64_decode_round_trips_every_byte_value_and_length() {
    let out = prints(
        "b64_roundtrip",
        &program(
            "    let mutable bad be 0
    for length in 0..40:
        let mutable bytes be Array[U8].new()
        for i in 0..length:
            bytes.push(((i * 37 + length * 11) % 256) as U8)
        let a, e1 be encoding.base64_decode(&encoding.base64_encode(&bytes))
        let b, e2 be encoding.base64_url_decode(&encoding.base64_url_encode(&bytes))
        let c, e3 be encoding.base64_url_decode(&encoding.base64_url_encode_padded(&bytes))
        if e1? or e2? or e3?:
            bad be bad + 1
        else:
            if encoding.hex_encode(&a) is not encoding.hex_encode(&bytes):
                bad be bad + 1
            if encoding.hex_encode(&b) is not encoding.hex_encode(&bytes):
                bad be bad + 1
            if encoding.hex_encode(&c) is not encoding.hex_encode(&bytes):
                bad be bad + 1
    print(bad)
    let mutable all be Array[U8].new()
    for i in 0..256:
        all.push(i as U8)
    let text be encoding.base64_encode(&all)
    let back, err be encoding.base64_decode(&text)
    dump(back, err)
",
        ),
    );
    let all: String = (0..256).map(|i| format!("{i:02x}")).collect();
    assert_eq!(out, format!("0\nok {all}\n"));
}

#[test]
fn base64_decode_refuses_malformed_input_and_says_where() {
    let out = prints(
        "b64_errors",
        &program(
            "    let a, e1 be encoding.base64_decode(\"TWF\")
    dump(a, e1)
    let b, e2 be encoding.base64_decode(\"TW=u\")
    dump(b, e2)
    let c, e3 be encoding.base64_decode(\"TWFu!\")
    dump(c, e3)
    let d, e4 be encoding.base64_decode(\"TWE\")
    dump(d, e4)
    let e, e5 be encoding.base64_decode(\"TWF=\")
    dump(e, e5)
    let f, e6 be encoding.base64_decode(\"TWFx\")
    dump(f, e6)
    let g, e7 be encoding.base64_decode(\"TR==\")
    dump(g, e7)
    let h, e8 be encoding.base64_decode(\"T\")
    dump(h, e8)
    let i, e9 be encoding.base64_decode(\"-_-_\")
    dump(i, e9)
    let j, e10 be encoding.base64_url_decode(\"-__-AAEC\")
    dump(j, e10)
    let k, e11 be encoding.base64_url_decode(\"+//+\")
    dump(k, e11)
    let l, e12 be encoding.base64_url_decode(\"Zg=\")
    dump(l, e12)
    let m, e13 be encoding.base64_url_decode(\"Zg\")
    dump(m, e13)
    let n, e14 be encoding.base64_decode(\"TWFu\\n\")
    dump(n, e14)
",
        ),
    );
    assert_eq!(
        out,
        "error missing padding at 3\n\
         error not a base64 character at 2\n\
         error a lone character cannot encode a byte at 4\n\
         error missing padding at 3\n\
         error non-zero trailing bits at 2\n\
         ok 4d6171\n\
         error non-zero trailing bits at 1\n\
         error a lone character cannot encode a byte at 0\n\
         error not a base64 character at 0\n\
         ok fbfffe000102\n\
         error not a base64 character at 0\n\
         error wrong amount of padding at 2\n\
         ok 66\n\
         error a lone character cannot encode a byte at 4\n"
    );
}

#[test]
fn hex_encodes_both_cases_and_decodes_either() {
    let out = prints(
        "hex",
        &program(
            "    let bytes be raw(\"00ff7fA5b6\")
    print(encoding.hex_encode(&bytes))
    print(encoding.hex_encode_upper(&bytes))
    let empty be raw(\"\")
    print(f\"[{encoding.hex_encode(&empty)}]\")
    let a, e1 be encoding.hex_decode(\"abc\")
    dump(a, e1)
    let b, e2 be encoding.hex_decode(\"0g\")
    dump(b, e2)
    let c, e3 be encoding.hex_decode(\"zz00\")
    dump(c, e3)
    let d, e4 be encoding.hex_decode(\"DeadBEEF\")
    dump(d, e4)
",
        ),
    );
    assert_eq!(
        out,
        "00ff7fa5b6\n00FF7FA5B6\n[]\nerror odd number of hex digits at 3\n\
         error not a hex digit at 1\nerror not a hex digit at 0\nok deadbeef\n"
    );
}

#[test]
fn utf8_and_code_points_round_trip_and_validate() {
    let out = prints(
        "utf8",
        &program(
            "    let text be \"héllo €😀\"
    let bytes be encoding.utf8_encode(&text)
    print(encoding.hex_encode(&bytes))
    let back, err be encoding.utf8_decode(&bytes)
    show(back, err)
    let points be encoding.code_points(&text)
    for p in points:
        write(f\"{p as Int} \")
    print(\"\")
    let again, e2 be encoding.from_code_points(&points)
    show(again, e2)
    let overlong, e3 be encoding.utf8_decode(&raw(\"41c080\"))
    show(overlong, e3)
    let surrogate, e4 be encoding.utf8_decode(&raw(\"eda080\"))
    show(surrogate, e4)
    let truncated, e5 be encoding.utf8_decode(&raw(\"6162e282\"))
    show(truncated, e5)
    let stray, e6 be encoding.utf8_decode(&raw(\"6180\"))
    show(stray, e6)
    let big, e7 be encoding.utf8_decode(&raw(\"f4908080\"))
    show(big, e7)
    let mutable bad be Array[U32].new()
    bad.push(65)
    bad.push(55296)
    let s, e8 be encoding.from_code_points(&bad)
    show(s, e8)
",
        ),
    );
    assert_eq!(
        out,
        "68c3a96c6c6f20e282acf09f9880\nok héllo €😀\n104 233 108 108 111 32 8364 128512 \n\
         ok héllo €😀\nerror invalid UTF-8 at 1\nerror invalid UTF-8 at 0\n\
         error truncated UTF-8 sequence at 2\nerror invalid UTF-8 at 1\n\
         error invalid UTF-8 at 0\nerror not a Unicode scalar value at 1\n"
    );
}

#[test]
fn utf16_converts_units_and_bytes_and_refuses_unpaired_surrogates() {
    let out = prints(
        "utf16",
        &program(
            "    let text be \"héllo €😀\"
    let units be encoding.utf16_encode(&text)
    for u in units:
        write(f\"{u as Int} \")
    print(\"\")
    let back, err be encoding.utf16_decode(&units)
    show(back, err)
    let le be encoding.utf16le_encode(&text)
    let big be encoding.utf16be_encode(&text)
    print(encoding.hex_encode(&le))
    print(encoding.hex_encode(&big))
    let l, e1 be encoding.utf16le_decode(&le)
    show(l, e1)
    let b, e2 be encoding.utf16be_decode(&big)
    show(b, e2)
    let odd, e3 be encoding.utf16le_decode(&raw(\"680069\"))
    show(odd, e3)
    let lone_high, e4 be encoding.utf16le_decode(&raw(\"68003dd8\"))
    show(lone_high, e4)
    let lone_low, e5 be encoding.utf16le_decode(&raw(\"00de6800\"))
    show(lone_low, e5)
    let swapped, e6 be encoding.utf16le_decode(&raw(\"3dd83dd8\"))
    show(swapped, e6)
    let mutable pair be Array[U16].new()
    pair.push(55357)
    pair.push(56832)
    let grin, e7 be encoding.utf16_decode(&pair)
    show(grin, e7)
    let mutable lone be Array[U16].new()
    lone.push(65)
    lone.push(55357)
    let x, e8 be encoding.utf16_decode(&lone)
    show(x, e8)
    let empty be encoding.utf16_encode(\"\")
    print(empty.length())
",
        ),
    );
    assert_eq!(
        out,
        "104 233 108 108 111 32 8364 55357 56832 \nok héllo €😀\n\
         6800e9006c006c006f002000ac203dd800de\n006800e9006c006c006f002020acd83dde00\n\
         ok héllo €😀\nok héllo €😀\nerror odd number of bytes at 3\n\
         error high surrogate at the end of the input at 2\n\
         error unpaired low surrogate at 0\n\
         error high surrogate not followed by a low one at 0\nok 😀\n\
         error high surrogate at the end of the input at 1\n0\n"
    );
}
