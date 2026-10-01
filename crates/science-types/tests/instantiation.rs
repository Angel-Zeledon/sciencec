//! A generic type instantiated in an **expression**, whose argument is not a
//! bare name: `Set[(Int, Int)].new()` and `Array[String?].new()`.
//!
//! The parser cannot tell `Array[Int]` from `xs[i]` and leaves one argument
//! as an `ExprKind::Index`; `science-resolve` reads it back as an
//! instantiation when the base names a type. It did that for a path, an index
//! of a path and a borrow, and answered `None` for a **tuple** and for a
//! **nullable** — which are the two spellings a type annotation has always
//! accepted — so the expression stayed an index *of a type*, was typed at the
//! error type with no diagnostic beside it, and reached the backend as
//! `SC0400`. A type annotation of the same type worked, which is what made it
//! confusing to find.
//!
//! The whole pipeline runs, for `tests/support`'s reason.

mod support;

fn local_ty(checked: &support::Checked, body: &str, name: &str) -> String {
    let body = checked.body(body);
    let def = checked
        .krate
        .defs
        .iter()
        .find(|def| def.name == name && body.local_ty(def.id).is_some())
        .unwrap_or_else(|| panic!("the body binds no local named `{name}`"));
    checked.render(body.local_ty(def.id).expect("the local has a type"))
}

/// The three spellings agree with the annotation form, and the type is not the
/// error type.
#[test]
fn a_tuple_or_a_nullable_may_be_a_type_argument_in_an_expression() {
    let checked = support::check(
        "\
def f():
    let pairs be Set[(Int, Int)].new()
    let bytes be Array[(U8, Int)].new()
    let names be Array[String?].new()
    let nested be Array[(Int, String?)].new()
    let keyed be Map[String, (Int, Int)].new()
",
    );
    checked.assert_clean();
    assert_eq!(local_ty(&checked, "f", "pairs"), "Set[(I64, I64)]");
    assert_eq!(local_ty(&checked, "f", "bytes"), "Array[(U8, I64)]");
    assert_eq!(local_ty(&checked, "f", "names"), "Array[String?]");
    assert_eq!(local_ty(&checked, "f", "nested"), "Array[(I64, String?)]");
    assert_eq!(local_ty(&checked, "f", "keyed"), "Map[String, (I64, I64)]");
}

/// What a value subscripted by a tuple or a presence test still is: an index.
/// The rewrite is gated on the base naming a type, so `table[(i, j)]` and
/// `flags[done?]` are not instantiations of anything.
#[test]
fn a_value_subscripted_by_a_tuple_or_a_presence_test_is_still_an_index() {
    let checked = support::check(
        "\
def f(table: Array[Int], done: Int?, i: Int):
    let a be table[(i, i)]
    let b be table[done?]
",
    );
    let indexes = checked.nodes("f").iter().filter(|(kind, _)| kind == "index").count();
    assert_eq!(indexes, 2, "both subscripts stay indexes: {:?}", checked.nodes("f"));
}

/// A type subscripted by something that is not a type is reported where it is
/// written, and not left to fail in the backend with nothing said.
#[test]
fn a_type_subscripted_by_an_expression_is_reported_by_the_resolver() {
    let checked = support::check_allowing_resolution_errors(
        "\
def f():
    let x be 1
    let a be Array[x + 1].new()
",
    );
    let codes: Vec<u16> = checked.resolution.iter().map(|d| d.code.0).collect();
    assert_eq!(codes, vec![211], "{:?}", checked.resolution.iter().map(|d| d.message.clone()).collect::<Vec<_>>());
}

/// A closure argument is typed against the callee's declared parameter with the
/// call's solved parameters put in, and not against the declaration as written.
/// `apply(three, n giving n + 1)` is `(I64) -> I64` and not `(T) -> T`, and
/// so is the same call with a literal where `three` is: nothing else in the
/// argument list fixes `T`, and the closure is the one argument that cannot be
/// written until it is known.
#[test]
fn a_closure_argument_is_typed_at_the_solved_parameter() {
    let checked = support::check(
        "\
def apply[T](x: T, f: (T) -> T) -> T:
    f(x)

def convert[T, U](x: T, f: (T) -> U) -> U:
    f(x)

def a():
    let three: Int be 3
    let r be apply(three, n giving n + 1)

def b():
    let r be apply(3, n giving n + 1)

def c():
    let r be convert(4, n giving n > 3)
",
    );
    checked.assert_clean();
    for body in ["a", "b", "c"] {
        let closures: Vec<String> = checked
            .nodes(body)
            .into_iter()
            .filter(|(kind, _)| kind == "closure")
            .map(|(_, ty)| ty)
            .collect();
        assert_eq!(closures.len(), 1, "{body}: {:?}", checked.nodes(body));
        assert!(!closures[0].contains('T'), "{body}: closure typed {}", closures[0]);
    }
    assert_eq!(local_ty(&checked, "a", "r"), "I64");
    assert_eq!(local_ty(&checked, "c", "r"), "Bool");
}
