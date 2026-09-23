# Contributing to QelloxHashV1

## Algorithm Semantics Are Frozen

The algorithm is frozen as of Candidate A (`cec81499936e7dea1087805ceafe57d21b0b21e085bb61d6d79b35e3bfdecec5`). All implementations must produce identical outputs for every canonical test vector.

## Welcome

- **Optimizations** that preserve exact outputs (faster CPU impl, optimized WGSL, better memory patterns)
- **New backends** (CUDA, OpenCL, Metal, C, Python, etc.) that pass all vectors
- **Hardware qualification results** as JSON conforming to `docs/qualification/qualification-result.schema.json`
- **Documentation** clarifications and corrections

## Not Welcome

- Changing test vectors to make an implementation pass
- Altering algorithm semantics (opcodes, program generator, mixing, finalization, constants)
- Unverified performance claims

## Setup

```bash
git clone https://github.com/Qellox/QelloxHashV1.git
cd QelloxHashV1
cargo test --workspace
```

## PR Process

1. Fork, branch, change
2. `cargo test --workspace` passes
3. `cargo clippy --workspace -- -D warnings` passes
4. `cargo fmt --check` passes
5. Submit PR with clear description

Contributions are licensed under Apache-2.0.
