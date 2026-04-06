#[test]
fn ui_compile_fail_cases() {
    let tests = trybuild::TestCases::new();

    tests.compile_fail("tests/ui/fail/*.rs");
    tests.pass("tests/ui/pass/*.rs");
}
