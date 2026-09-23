# How QelloxHashV1 Works

Plain-English explanation for GPU/mining engineers, a single-nonce walkthrough, hardware design rationale, and qualification caveats. Exact normative definition lives in [SPEC.md](SPEC.md).

---

## 1. Plain-English Explanation

### What does QelloxHashV1 make the GPU do?

QelloxHashV1 is a proof-of-work algorithm that forces a GPU to execute a randomly generated program, then read data from memory in a way that cannot be parallelized. Here's what happens for every single nonce you try:

#### Step 1: Generate a unique program (CPU, ~15 SHA-256 hashes)

Your nonce and block header are hashed together to produce a 32-byte "seed." From that seed, a unique 112-instruction program is generated. Every nonce gets a different program. The program is deterministic — the same nonce always produces the same program.

The program uses 23 different instruction types: integer add, subtract, multiply, XOR, rotate, bit count, byte shuffle, memory loads, memory stores, and cross-lane register reads. The instruction mix is weighted so all types appear roughly proportionally.

#### Step 2: Initialize registers and scratchpad (CPU, ~2560 SHA-256 hashes)

Each of the 32 "lanes" (think: one lane per GPU thread in a warp) gets its own set of 32 registers. All 1024 registers are initialized from the seed using SHA-256 — every register gets a unique initial value.

A 48 KiB "scratchpad" is also initialized from the seed. This is shared memory that all lanes can read and write.

#### Step 3: Execute the program 4 times (GPU, pure integer work)

The GPU executes the 112-instruction program 4 times (4 "passes"). Each instruction operates on all 32 lanes simultaneously — this maps naturally to GPU SIMD/SIMT execution.

The program does integer arithmetic, bitwise operations, rotations, multiplies, and memory access. There is no floating point anywhere.

#### Step 4: Read from memory with dependencies (GPU, the critical part)

After each pass, every lane performs 8 reads from the scratchpad. The key design: **each read's address depends on the value returned by the previous read**. You cannot prefetch or pipeline these reads because you don't know where the next one will go until the current one completes.

This is the mechanism that limits the advantage of high-bandwidth memory (HBM). Whether you have 500 GB/s or 2000 GB/s, you still have to wait for each read to complete before starting the next.

#### Step 5: Mix between lanes (GPU, subgroup operations)

After each pass, lanes exchange register data with other lanes using a shuffle map, then perform a "butterfly" mixing pattern within groups of 8 lanes. This maps to GPU subgroup/wavefront operations.

#### Step 6: Reduce and finalize (CPU, ~2 SHA-256 hashes)

After 4 passes, all 1024 registers and the first 256 words of the scratchpad are hashed together to produce a 32-byte "work digest." That digest is hashed again with a domain separator to produce the final 256-bit proof-of-work digest.

If this digest is less than or equal to the target, you've found a valid block.

### Summary

QelloxHashV1 makes the GPU: (1) generate a random program, (2) execute it 4 times with integer-only operations, (3) perform 32 serial dependent memory reads, (4) mix data between lanes, and (5) hash the result. The dependent memory reads are the key mechanism that prevents specialized hardware from dominating through raw bandwidth.

---

## 2. Single-Nonce Walkthrough

Complete trace of one nonce through the entire algorithm.

**Input:**
- Header: 80 zero bytes (`0x00` × 80)
- Nonce: `0` (u64)
- Epoch: `0` (u32)

**Output:**
- PoW Digest: `fce8c5b78c10ca06908137b318d49898509be31ab6214c212ba8dc40b0003718`

### Step 1: Mining seed derivation

**Formula:** `SHA-256("QelloxHashV1/seed/v1" || header[80] || le64(nonce))`

**Input bytes to SHA-256:**
```
Domain:  51 65 6c 6c 6f 78 48 61 73 68 56 31 2f 73 65 65 64 2f 76 31
         (20 bytes: "QelloxHashV1/seed/v1")
Header:  00 00 00 00 00 00 00 00 ... (80 zero bytes)
Nonce:   00 00 00 00 00 00 00 00 (8 bytes, little-endian)
```

**Total input:** 108 bytes

**Output (mining seed):**
```
1590ff90d8a42d7a9f85def3d6367b1a635953f9df452045f6b60ce41013f6e7
```

### Step 2: Program generation

#### 2.1 Keystream expansion

15 SHA-256 blocks generated:
```
for i in 0..15:
    keystream[i*32..(i+1)*32] = SHA-256("QelloxHashV1/program-expand/v1" || seed || le32(i))
```

Total keystream: 480 bytes (15 × 32).

#### 2.2 Instruction generation

For each of 112 positions, 4 bytes of keystream are read as a little-endian u32 (`raw`). An opcode is selected using the weighted group algorithm, and operand fields are extracted from `raw`.

#### 2.3 Guaranteed operations injection

After random generation, specific positions are overwritten:

| Position | Opcode | Purpose |
|----------|--------|---------|
| 0 | DATASET_LOAD (0x12) | Pass 0 boundary |
| 1 | SCRATCH_LOAD (0x10) | Pass 0 scratchpad read |
| 2 | LANE_SHUFFLE (0x13) | Pass 0 cross-lane |
| 3 | SCRATCH_STORE (0x11) | Pass 0 scratchpad write |
| 28 | DATASET_LOAD (0x12) | Pass 1 boundary |
| 30 | LANE_SHUFFLE (0x13) | Pass 1 cross-lane |
| 56 | DATASET_LOAD (0x12) | Pass 2 boundary |
| 58 | LANE_SHUFFLE (0x13) | Pass 2 cross-lane |
| 84 | DATASET_LOAD (0x12) | Pass 3 boundary |
| 86 | LANE_SHUFFLE (0x13) | Pass 3 cross-lane |

#### 2.4 Generated program (first 8 instructions)

```
[ 0] DATASET_LOAD   R21 = R24 + R0 + 0           (guaranteed)
[ 1] SCRATCH_LOAD   R31 = scratchpad[R5 + 0]      (guaranteed)
[ 2] LANE_SHUFFLE   R21 = lane[(lane+1+0)%32].R24 (guaranteed)
[ 3] SCRATCH_STORE  scratchpad[R0 + 4] ^= R31     (guaranteed)
[ 4] ROTL32         R27 = R0.rotate_left(R2 & 31)
[ 5] ROTR32         R17 = R22.rotate_right(R26 & 31)
[ 6] ROTL_IMM32     R17 = R5.rotate_left(12)
[ 7] MUL_LO32       R8 = (R29 * R17) as u32
```

Instructions 4–111 are pseudo-randomly generated from the keystream.

#### 2.5 Cross-lane map

```
map[0] = 18, map[1] = 21, map[2] = 17, map[3] = 1,
map[4] = 9,  map[5] = 30, map[6] = 10, map[7] = 25, ...
```

No lane maps to itself (enforced by generation).

### Step 3: State initialization

#### 3.1 Register initialization

1024 SHA-256 invocations (32 lanes × 32 registers):
```
registers[l][r] = SHA-256("QelloxHashV1/reg-init/v1" || seed || le32(l) || le32(r))[0..4]
```

**Sample values:**

| Lane | R0 | R1 | R2 | R3 |
|------|-----|-----|-----|-----|
| 0 | 4157579017 (0xf7cf9f09) | 2086940686 | 2153633754 | 2139409018 |
| 1 | 1534094484 | 3321374034 | 3565261794 | 1866098535 |
| 31 | 2057250812 | 3297641514 | 3474897040 | 572141948 |

#### 3.2 Scratchpad initialization

1536 SHA-256 invocations (12288 words / 8 words per hash):
```
for i in 0..1536:
    hash = SHA-256("QelloxHashV1/scratch-init/v1" || seed || le32(i))
    scratchpad[i*8 .. i*8+8] = hash interpreted as 8 little-endian u32 values
```

**Sample values:**
```
scratchpad[0] = 4083804960
scratchpad[1] = 4106660227
scratchpad[2] = 1509381196
scratchpad[3] = 2053133587
```

### Step 4: Execution (4 passes)

Each pass consists of:
1. Execute all 112 instructions across all 32 lanes
2. Cross-lane mixing (shuffle + butterfly)
3. Dataset reads with dependency chains

#### Pass 0

**Phase 1: Instruction execution**

All 112 instructions execute across all 32 lanes simultaneously. For example, instruction [0] (DATASET_LOAD):
```
For each lane l in 0..31:
    R21[l] = R24[l] + R0[l] + 0
```

This is address computation only — the actual memory read happens in Phase 3.

**Phase 2: Cross-lane mixing**

1. Each lane reads its peer's registers (peer = cross_lane_map[lane])
2. XOR-rotate mix: `R[lane][reg] ^= R[peer][reg].rotate_left(reg & 31)`
3. 8-lane butterfly: 3 stages of add-rotate-left(7) / sub-rotate-left(13)

**Phase 3: Dataset reads**

For each lane, 8 serial dependent reads:
```
chain = R[0] + R[4] + 0 * 0x01000193
for r in 0..7:
    index = chain * 0x01000193 + 0x811C9DC5
    value = scratchpad[index % 12288]
    R[(r*4+lane+0) % 32] = (R[(r*4+lane+0)%32] + value).rotate_left((r+3)&31)
    chain = (value + R[(r+1)%32]) * 0x01000193
    scratchpad[index % 12288] ^= R[(r+2)%32]
```

#### Passes 1, 2, 3

Identical structure to Pass 0, but:
- State (registers + scratchpad) persists from previous pass
- Pass index affects dataset read initialization: `chain += pass * 0x01000193`
- Pass index affects destination register: `dst_reg = (r*4+lane+pass) % 32`

### Step 5: State reduction

After all 4 passes, the final state is reduced to a 32-byte work digest:

```
// Scratchpad hash (first 256 words only)
sp_hash = SHA-256("QelloxHashV1/scratchpad/v1" || scratchpad[0..256])

// Work digest
work_digest = SHA-256("QelloxHashV1/reduce/v1" || registers[0..1024] || sp_hash)
```

**Note:** Only the first 256 of 12,288 scratchpad words are included in the reduction. The scratchpad's influence is primarily through its effect on register state during execution.

### Step 6: Final digest

```
pow_digest = SHA-256("Qellox/QelloxHashV1" || work_digest)
```

**Input:** 19 bytes (domain) + 32 bytes (work_digest) = 51 bytes

**Output:**
```
fce8c5b78c10ca06908137b318d49898509be31ab6214c212ba8dc40b0003718
```

### Verification

```rust
let header = [0x00u8; 80];
let nonce: u64 = 0;
let result = qelloxhash_v1(&header, nonce);
assert_eq!(hex::encode(result), "fce8c5b78c10ca06908137b318d49898509be31ab6214c212ba8dc40b0003718");
```

### Cross-checks

| Input | PoW Digest |
|-------|------------|
| header=0x00×80, nonce=0 | `fce8c5b78c10ca06908137b318d49898509be31ab6214c212ba8dc40b0003718` |
| header=0x00×80, nonce=1 | `794f9d7586a64079b4a9efbfc459a4c56e78e6bab1c00b12e3b0e72db0c2bb62` |
| header=0xFF×80, nonce=0 | `14c32ada6b3db609eb03628407a6cdb1d04b9d2321b2ca0024ddd106239d7b64` |

All different, confirming nonce and header sensitivity.

### SHA-256 invocation count per hash

| Stage | Count |
|-------|-------|
| Mining seed | 1 |
| Program keystream | 15 |
| Register initialization | 1024 |
| Scratchpad initialization | 1536 |
| Scratchpad hash (reduction) | 1 |
| Work digest (reduction) | 1 |
| Final digest | 1 |
| **Total** | **2579** |

Note: Program generation and state initialization are pure SHA-256. The VM execution (passes) uses zero SHA-256 — it is pure integer arithmetic.

---

## 3. Hardware Design Rationale

All hardware performance numbers in this section are **MODELED — NOT HARDWARE VERIFIED**.

### Why gaming GPUs should handle this well

1. **Integer ALUs**: Gaming GPUs have thousands of integer ALUs. QelloxHashV1 is 100% integer work — no FP64, no tensors, no AI units.
2. **Register pressure**: 32 registers per lane × 32 lanes = 4 KiB per wavefront. This fits comfortably in GPU register files without spilling to memory.
3. **Shared memory**: The 48 KiB scratchpad fits within per-SM shared memory (100–164 KiB on NVIDIA, 64 KiB on AMD). 48 KiB was chosen primarily to fit AMD LDS (64 KiB/CU) while allowing 2+ workgroups per NVIDIA SM.
4. **Subgroup operations**: The 8-lane butterfly mixing maps to native warp/wavefront shuffle instructions (NVIDIA SHFL, AMD DPP/ds_swizzle).
5. **Lane width**: 32 lanes maps 1:1 to NVIDIA warps and RDNA 2/3 wavefronts.

### Why CPUs should perform poorly

1. **No parallelism**: A CPU has 8–16 cores. A GPU has thousands of ALUs. The 32 lanes execute in parallel on a GPU but sequentially on a CPU.
2. **No subgroup operations**: CPUs must emulate lane shuffles using memory loads/stores, which is much slower.
3. **Memory latency**: The dependent read chain is equally punishing for CPUs and GPUs, but GPUs have thousands of other threads to work on while waiting.

### What specifically reduces HBM advantage

The 8 dependent reads per pass. Each read's address is computed from the previous read's result. HBM has much higher bandwidth than GDDR6, but similar latency (~400–600 cycles). Since the reads are serial, bandwidth doesn't help — only latency matters. This levels the playing field between HBM-equipped mining cards (e.g., CMP 170HX) and GDDR6 gaming cards. FP64-heavy compute cards are also disadvantaged because the algorithm uses zero FP operations.

### What challenges ASICs

1. **Dynamic programs**: Every nonce generates a different 112-instruction program with 23 possible opcodes. An ASIC must either implement all 23 opcodes (becoming essentially a general-purpose processor) or skip some (producing wrong results).
2. **Cross-lane mixing**: The butterfly pattern requires a full crossbar between 8 lanes, which is expensive in silicon.
3. **Dataset requirement**: The ~2 GiB dataset must be accessible, requiring either expensive on-chip SRAM or external memory.

Estimated ASIC advantage: **10–50× hash/W** (moderate resistance — ASICs are possible but must be substantially programmable).

### What challenges FPGAs

1. **Block RAM**: The 48 KiB scratchpad consumes limited BRAM resources. Multiple mining instances multiply this.
2. **Dynamic instruction decode**: Supporting 23 opcodes requires a real instruction decoder, consuming LUTs.
3. **Lane interconnect**: The cross-lane mixing requires routing between 8 processing elements.

Estimated FPGA advantage: **2–5× hash/$**, limited by BRAM and instruction diversity.

### Parameter justification

| Parameter | Value | Rationale | Confidence |
|-----------|-------|-----------|------------|
| Lanes | 32 | GPU warp/wavefront size | REASONABLE |
| Registers/lane | 32 | 5-bit encoding, fits GPU register files | REASONABLE |
| Program length | 112 instructions | Divisible by 4 (28 per pass slot); no deep justification | ARBITRARY |
| Opcodes | 23 | Covers arithmetic, bitwise, rotate, multiply, bit manipulation, memory, cross-lane | REASONABLE |
| Scratchpad | 48 KiB | Fits AMD LDS (64 KiB/CU) and NVIDIA shared memory | REASONABLE |
| Dataset | ~2 GiB | Prevents CPU mining, fits consumer GPU VRAM (8+ GB) | REASONABLE |
| Passes | 4 | Reasonable work multiplier; 2–8 would work similarly | ARBITRARY |
| Dependent reads/pass | 8 | Core memory-level-parallelism-limiting mechanism | STRONG |
| Subgroup size | 8 | Maps to GPU subgroup sizes (32 ÷ 4) | REASONABLE |
| Dynamic program | (design) | Core ASIC-resistance mechanism | STRONG |
| SHA-256 finalizer | (design) | Collision/preimage resistance + domain separation | STRONG |

### PCIe behavior

- Steady-state mining traffic: ~600 bytes/job (header+nonce 88, program 448, cross-lane map 32, result 32).
- At 1 MH/s: ~600 MB/s (fits PCIe 3.0 x1 at 985 MB/s). At 5 MH/s: ~3 GB/s (requires PCIe 3.0 x4).
- Dataset initialization: ~2 GiB one-time per epoch (~250 s on PCIe 3.0 x1, ~16 s on x16).

### Verification cost

Node verification does **not** require the full 2 GiB dataset — only the ~16 MiB light cache. Measured CPU reference verification: ~194 µs per hash (release mode).

---

## 4. Qualification Caveats

### Status

- **No physical hardware testing has been completed.** All performance numbers are modeled.
- CPU/GPU equivalence has been validated in software (2215/2215 deterministic test cases) but not on physical GPUs.
- Hardware qualification is required before any consensus activation consideration.

### Threat-model limitations

- **ASIC resistance is moderate.** An ASIC could achieve ~10–50× hash/W by eliminating instruction fetch overhead and optimizing memory. The 23-opcode dynamic program and dependent reads force substantial programmability.
- **FPGA resistance is moderate.** ~2–5× hash/$ estimated, limited by BRAM and instruction diversity.
- **HBM advantage** is largely negated by 8 serial dependent reads per pass, but not fully eliminated.
- **Program generator** not adversarially tested at scale.
- **Cross-architecture determinism** not validated on physical hardware.

### Benchmark methodology rules

| Rule | Requirement |
|------|-------------|
| Warmup | Minimum 60 seconds (results discarded) |
| Measurement | Minimum 600 s (quick), 1800 s preferred, 3600 s for stability certification |
| Rate reporting | Sustained rate = total_hashes / total_time, **NOT** peak per-second rate |
| Thermal | No active throttling during measurement |
| Algorithm | Frozen candidate only, no auto-tuning |
| Stability | Rate drift <10% across runs; zero digest mismatches; zero device losses |

### Qualification acceptance criteria

A qualification run is **ACCEPTED** only if ALL are true:

1. Candidate manifest digest matches `cec81499936e7dea1087805ceafe57d21b0b21e085bb61d6d79b35e3bfdecec5`
2. CPU/GPU equivalence: 0 mismatches out of 1000+ test cases
3. No GPU validation errors
4. No digest mismatches during benchmark
5. No invalid solutions generated
6. Stability: rate drift <10% across runs
7. No uncorrected errors

**REJECTED** on: digest mismatch, equivalence failure, any mismatch/invalid solution, GPU validation errors, or rate drift >20% (thermal throttling/instability).

Qualification command:

```bash
cargo run -p qelloxhash-bench -- qualify \
  --device 0 --seconds 600 --warmup-seconds 60 \
  --candidate docs/qualification/CANDIDATE_A.json \
  --output result.json
```

Result schema: `docs/qualification/qualification-result.schema.json`. If power is not measured, mark `power_source: "not-measured"` and exclude from efficiency calculations.

### Model-vs-measured tracking

`docs/qualification/model-vs-measured.json` records modeled vs measured throughput per architecture. Do NOT recalibrate the simulator to match measurements; the purpose is to track prediction accuracy, not force agreement.

### Minimum evidence before consensus activation

1. At least one AMD gaming GPU qualified (e.g., RX 7700 XT)
2. At least one NVIDIA gaming GPU qualified (e.g., RTX 3060 or 3070)
3. CPU/GPU equivalence proven on both
4. Sustained stability (60+ minutes)
5. Real power data available
6. Real PCIe/riser test completed
7. At least one specialized card tested (e.g., CMP 170HX)
8. No serious security shortcuts discovered
9. Node verification cost acceptable (<1 ms)

### Redesign triggers

Do NOT change the algorithm unless: real hardware contradicts the architecture model significantly; CPU/GPU equivalence fails due to design ambiguity; verification is too expensive (>>1 ms per hash); a meaningful ASIC/FPGA shortcut is discovered; specialized hardware dominates physically (hash/$ not hash rate); or a major compiler/backend pathology exists. If any trigger occurs, document the failure before changing anything.
