//! Decision 8's query boundary: the strongly connected component.
//!
//! > *"The query boundary for incremental recompilation is the strongly
//! > connected component of the call graph, not the function."*
//!
//! and §6.2's order, which the components come out in: *"reverse topological
//! order of the call graph, leaves first"*.

mod support;

use support::lower;

#[test]
fn a_call_is_an_edge_and_a_constructor_is_not() {
    let source = concat!(
        "choice Shape:\n",
        "    Round\n",
        "    Sized(Int)\n",
        "\n",
        "def leaf(n: Int) -> Int:\n",
        "    n\n",
        "\n",
        "def root() -> Int:\n",
        "    let s be Sized(1)\n",
        "    leaf(2)\n",
    );
    let lowered = lower(source);
    let graph = lowered.call_graph();
    let root = lowered.def("root", science_resolve::hir::DefKind::Fn);
    let leaf = lowered.def("leaf", science_resolve::hir::DefKind::Fn);
    assert_eq!(graph.callees(root), &[leaf]);
    let sized = lowered.def("Sized", science_resolve::hir::DefKind::Variant);
    assert!(
        !graph.callees(root).contains(&sized),
        "a variant constructor became a call-graph node"
    );
}

#[test]
fn components_come_out_leaves_first() {
    let source = concat!(
        "def leaf(n: Int) -> Int:\n",
        "    n\n",
        "\n",
        "def middle(n: Int) -> Int:\n",
        "    leaf(n)\n",
        "\n",
        "def top(n: Int) -> Int:\n",
        "    middle(n)\n",
    );
    let lowered = lower(source);
    let graph = lowered.call_graph();
    let order: Vec<String> = graph
        .components()
        .iter()
        .map(|scc| {
            scc.iter()
                .map(|def| lowered.krate.defs.get(*def).name.to_string())
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect();
    assert_eq!(order, vec!["leaf", "middle", "top"]);
}

/// §6.2 needs SCCs for mutual recursion, and Decision 8 reuses them. This is
/// the case that makes them one component rather than three.
#[test]
fn mutual_recursion_is_one_component() {
    let source = concat!(
        "def even(n: Int) -> Bool:\n",
        "    odd(n)\n",
        "\n",
        "def odd(n: Int) -> Bool:\n",
        "    even(n)\n",
    );
    let lowered = lower(source);
    let graph = lowered.call_graph();
    let components = graph.components();
    assert_eq!(components.len(), 1, "{components:?}");
    assert_eq!(components[0].len(), 2);
}

/// §14: *"a mutually recursive descent parser is one strongly connected
/// component, the Science compiler will contain one, and Decision 8
/// re-analyses the whole component on any change inside it."* The cost is real
/// and this is what it looks like.
#[test]
fn a_recursive_descent_shape_is_one_component_and_that_is_the_cost() {
    let source = concat!(
        "def expression(n: Int) -> Int:\n",
        "    term(n)\n",
        "\n",
        "def term(n: Int) -> Int:\n",
        "    factor(n)\n",
        "\n",
        "def factor(n: Int) -> Int:\n",
        "    expression(n)\n",
        "\n",
        "def parse(n: Int) -> Int:\n",
        "    expression(n)\n",
    );
    let lowered = lower(source);
    let graph = lowered.call_graph();
    let components = graph.components();
    assert_eq!(components.len(), 2, "{components:?}");
    let cycle = components.iter().find(|scc| scc.len() == 3).expect("the cycle");
    assert_eq!(cycle.len(), 3, "three functions, one cache entry");
    // And the caller is analysed after it.
    let parse = lowered.def("parse", science_resolve::hir::DefKind::Fn);
    assert_eq!(components.last().unwrap(), &vec![parse]);
}

#[test]
fn direct_recursion_is_a_component_of_one_that_knows_it_is_a_cycle() {
    let source = "def f(n: Int) -> Int:\n    f(n)\n";
    let lowered = lower(source);
    let graph = lowered.call_graph();
    let f = lowered.def("f", science_resolve::hir::DefKind::Fn);
    assert!(graph.is_directly_recursive(f));
    assert_eq!(graph.component_of(f), Some(vec![f]));
}

/// A callee with no body here is not a node: Decision 7 reads a cross-crate
/// signature summary rather than analysing a body it does not have.
#[test]
fn a_callee_with_no_body_is_not_a_node() {
    let lowered = lower("def f(n: Int):\n    print(n)\n");
    let graph = lowered.call_graph();
    assert_eq!(graph.functions().count(), 1, "`print` became a node");
    assert_eq!(graph.components().len(), 1);
}

/// Determinism: the components are a cache key, and a key that moves for no
/// reason is a cache that never hits.
#[test]
fn the_components_are_the_same_on_two_runs() {
    let source = concat!(
        "def a(n: Int) -> Int:\n    b(n)\n\n",
        "def b(n: Int) -> Int:\n    c(n)\n\n",
        "def c(n: Int) -> Int:\n    a(n)\n\n",
        "def d(n: Int) -> Int:\n    b(n)\n",
    );
    let first = lower(source);
    let second = lower(source);
    let names = |lowered: &support::Lowered| {
        lowered
            .call_graph()
            .components()
            .iter()
            .map(|scc| {
                scc.iter()
                    .map(|def| lowered.krate.defs.get(*def).name.to_string())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&first), names(&second));
}
