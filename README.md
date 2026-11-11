# todoing (Rust port)

A behavior-preserving Rust rewrite of [the original Python todoing](https://github.com/bllyanos/todoing).

The original Python repository is the behavioral reference. This repository changes the implementation language only; command semantics, task storage, index behavior, and user-visible output are compatibility requirements.

## Build and test

```bash
cargo build --release
cargo test
```

## Status

Port in progress. Compatibility is not yet certified against the original implementation.
