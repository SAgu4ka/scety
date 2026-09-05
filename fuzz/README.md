# Scety fuzzing

Install cargo-fuzz and nightly Rust, then run:

```bash
cargo fuzz run host-router
```

The first target exercises wildcard classification and matching, including malformed UTF-8-free
string inputs, deep labels, and unusual wildcard placement.
