#[test]
fn ui_compile_fail_cases() {
    let tests = trybuild::TestCases::new();

    tests.compile_fail("tests/dinoco/ui/fail/*.rs");
    tests.pass("tests/dinoco/ui/pass/*.rs");
}
