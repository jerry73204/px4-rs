//! Compile-fail tests for `#[px4::main]`. Each `.rs` under
//! `tests/trybuild/fail/` documents one shape of misuse; a matching
//! `.stderr` snapshot pins the diagnostic so a regression in macro
//! error spans gets caught.

#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/fail/*.rs");
}
