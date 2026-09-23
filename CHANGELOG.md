# Changelog

## v0.9.0 (2026-09-22)

Pre-release for public research and hardware qualification.

Included:
- Formal specification with pseudocode and glossary (`docs/SPEC.md`)
- Plain-English guide, walkthrough, and hardware rationale (`docs/HOW_IT_WORKS.md`)
- CPU reference implementation (safe Rust)
- wgpu/WGSL GPU backend
- Canonical test vectors
- Fuzz tests (11) and unit tests (34)
- Benchmark and qualification CLI
- Architecture simulator
- Hardware qualification manifest, schema, and model tracking

Manifest finalization: Candidate A is intentionally exact. The `flags` field (unused by execution), `DATASET_LOAD` naming (address computation only), and `CROSS_LANE_ROUNDS` constant (informational) do not affect outputs.

Status: algorithm frozen, hardware qualification pending, independent implementation pending, not active consensus.
