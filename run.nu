#!/usr/bin/env nu

# Define the function to run the tests
def run_tests [features: list<string>] {
    RUSTFLAGS="-Awarnings" RUST_LOG=debug cargo test dict --release --features ($features | str join ",") -- --show-output
}

# Call the function with the captured arguments
export def main [...rest: string] {
    run_tests $rest
}
