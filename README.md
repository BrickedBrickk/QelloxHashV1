# QelloxHashV1

Consumer-GPU proof-of-work algorithm designed for equal AMD/NVIDIA performance and economic resistance to specialized mining hardware.

> **Not active Qellox consensus.** Frozen for research and hardware qualification.

## Design

- 32-lane × 32-register integer VM with 112-instruction dynamic programs (23 opcodes, 7 groups)
- 48 KiB scratchpad (fits GPU shared memory) and ~2 GiB dataset with 8 serial dependent reads per pass
- Cross-lane butterfly mixing (8-lane subgroups), SHA-256 finalization with domain separation
- Integer-only, wrapping arithmetic, no undefined behavior

## Status

- v0.9.0 prerelease
- Specification frozen (Candidate A, manifest SHA `cec81499...`)
- CPU reference implementation (safe Rust, `#![forbid(unsafe_code)]`) and wgpu/WGSL GPU backend
- 34 unit tests + 11 fuzz tests
- Hardware qualification pending; independent implementation pending

## Build & Test

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## Bench & Qualify

```bash
cargo run -p qelloxhash-bench -- cpu --seconds 60
cargo run -p qelloxhash-bench -- gpu --device 0 --seconds 60
cargo run -p qelloxhash-bench -- qualify --device 0 --seconds 600 --candidate docs/qualification/CANDIDATE_A.json --output result.json
```

GPU commands require physical hardware. Not part of CI.

## Security

QelloxHashV1 targets economic resistance to specialized mining hardware (ASICs, FPGAs, compute GPUs) while favoring consumer gaming GPUs.

Known limitations:

- **No hardware testing completed.** All performance numbers are modeled.
- **ASIC resistance is moderate** (~10–50× hash/W possible). The 23-opcode dynamic program and dependent reads force substantial programmability.
- **FPGA resistance is moderate** (~2–5× hash/$ estimated), limited by BRAM and instruction diversity.
- **HBM advantage** largely negated by 8 serial dependent reads per pass, but not fully eliminated.
- **Program generator** not adversarially tested at scale.
- **Cross-architecture determinism** not validated on physical hardware.

Do not open public issues for security vulnerabilities. Contact maintainers directly with reproduction steps and impact analysis.

## Hardware Qualification

Results must reference the frozen candidate manifest `docs/qualification/CANDIDATE_A.json` (SHA-256 `cec81499936e7dea1087805ceafe57d21b0b21e085bb61d6d79b35e3bfdecec5`). Any parameter change requires a new candidate ID.

A run is accepted only with: matching manifest digest, 0 CPU/GPU equivalence mismatches (1000+ cases), no digest mismatches or invalid solutions, no GPU validation errors, rate drift <10%, and no uncorrected errors. Report sustained rate (`total/time`), never peak; require ≥60 s warmup and ≥600 s measurement; mark power `not-measured` unless an external meter or telemetry was used. Results conform to `docs/qualification/qualification-result.schema.json`.

Full caveats, methodology, and activation prerequisites: [docs/HOW_IT_WORKS.md](docs/HOW_IT_WORKS.md).

## Documentation

- [docs/SPEC.md](docs/SPEC.md) — Formal specification, opcode semantics, pseudocode, glossary
- [docs/HOW_IT_WORKS.md](docs/HOW_IT_WORKS.md) — Plain-English explanation, walkthrough, hardware rationale, qualification caveats
- [vectors/test_vectors.json](vectors/test_vectors.json) — Canonical test vectors
- [CHANGELOG.md](CHANGELOG.md) · [CONTRIBUTING.md](CONTRIBUTING.md)

## License

Apache-2.0. See [LICENSE](LICENSE).
