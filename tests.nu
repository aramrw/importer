#!/usr/bin/env nu

def run_tests [ test_names: list<string> ] {
    let features = ["full"]
    let feature_str = ($features | str join ",")
    
    # with-env ensures RUSTFLAGS is only active for this specific command
    with-env { RUSTFLAGS: "-Awarnings", RUST_LOG: "debug" } {
        cargo nextest run --release --features $feature_str -- ...$test_names --no-capture
    }
}

export def main [...rest: string] {
    run_tests ["dict", "unit_test::import_zip"]
}
