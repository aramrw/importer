#!/usr/bin/env nu

def run_tests [
    test_names: list<string>,
] {
    let features = ["full"]
    let feature_str = ($features | str join ",")
    # Place test_names AFTER the -- to pass them to the test runner
    RUSTFLAGS="-Awarnings" RUST_LOG=debug cargo test --release --features $feature_str -- ...$test_names --show-output
}

export def main [...rest: string] {
    run_tests ["dict", "unit_test::import_zip"]
}
