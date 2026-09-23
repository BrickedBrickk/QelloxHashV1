use sha2::{Digest, Sha256};

use crate::types::{Instruction, Program};
use crate::LANES;

/// Number of instructions in the default program.
pub const DEFAULT_PROGRAM_LENGTH: usize = 112;

/// Opcode definitions for the QelloxHashV1 VM.
///
/// All opcodes operate on u32 values with wraparound semantics.
/// There is no undefined behavior in any operation.
pub mod opcode {
    /// dst = src_a.wrapping_add(src_b)
    pub const ADD32: u8 = 0x01;
    /// dst = src_a.wrapping_sub(src_b)
    pub const SUB32: u8 = 0x02;
    /// dst = src_a ^ src_b
    pub const XOR32: u8 = 0x03;
    /// dst = src_a & src_b
    pub const AND32: u8 = 0x04;
    /// dst = src_a | src_b
    pub const OR32: u8 = 0x05;
    /// dst = !src_a
    pub const NOT32: u8 = 0x06;
    /// dst = src_a.rotate_left(src_b & 31)
    pub const ROTL32: u8 = 0x07;
    /// dst = src_a.rotate_right(src_b & 31)
    pub const ROTR32: u8 = 0x08;
    /// dst = low 32 bits of (src_a * src_b)
    pub const MUL_LO32: u8 = 0x09;
    /// dst = high 32 bits of (src_a * src_b)
    pub const MUL_HI32: u8 = 0x0A;
    /// dst = src_a.wrapping_add(src_b.wrapping_mul(src_c)) where src_c = imm5 interpreted
    pub const MADD32: u8 = 0x0B;
    /// dst = popcount(src_a)
    pub const POPCNT32: u8 = 0x0C;
    /// dst = leading zeros of src_a (32 if zero)
    pub const CLZ32: u8 = 0x0D;
    /// dst = (src_a & src_c) | (src_b & !src_c) where src_c = register from imm5
    pub const BITSELECT: u8 = 0x0E;
    /// Byte-level permutation of src_a using src_b as control
    pub const BYTE_PERMUTE: u8 = 0x0F;
    /// dst = scratchpad[index % scratchpad_words] where index = src_a wrapping_add imm5
    pub const SCRATCH_LOAD: u8 = 0x10;
    /// scratchpad[index % scratchpad_words] = src_a where index = src_b wrapping_add imm5
    pub const SCRATCH_STORE: u8 = 0x11;
    /// dst = dataset_load(index) where index derived from src_a and lane state
    pub const DATASET_LOAD: u8 = 0x12;
    /// Cross-lane shuffle: dst = lane_peer's src_a register
    pub const LANE_SHUFFLE: u8 = 0x13;
    /// dst = src_a.wrapping_add(imm5 as u32)  (immediate add)
    pub const ADD_IMM32: u8 = 0x14;
    /// dst = src_a ^ (imm5 as u32)
    pub const XOR_IMM32: u8 = 0x15;
    /// dst = src_a.rotate_left(imm5 as u32)
    pub const ROTL_IMM32: u8 = 0x16;
    /// dst = src_a.wrapping_mul(src_b).rotate_left(imm5 & 31)
    pub const MUL_ROT32: u8 = 0x17;
}

/// Generates a deterministic microprogram from a 32-byte seed.
#[must_use]
pub fn generate_program(seed: &[u8; 32]) -> Program {
    generate_program_with_length(seed, DEFAULT_PROGRAM_LENGTH)
}

/// Generates a program with a specified instruction count.
#[must_use]
pub fn generate_program_with_length(seed: &[u8; 32], length: usize) -> Program {
    // Expand seed into enough bytes for instruction generation + cross-lane map
    let expand_bytes = length * 4 + LANES;
    let expand_words = expand_bytes.div_ceil(32);
    let mut keystream = vec![0u8; expand_words * 32];
    expand_for_program(seed, &mut keystream);

    let mut instructions = Vec::with_capacity(length);
    let mut opcode_histogram = [0u32; 24]; // Track opcode distribution

    for i in 0..length {
        let offset = i * 4;
        let raw = u32::from_le_bytes([
            keystream[offset],
            keystream[offset + 1],
            keystream[offset + 2],
            keystream[offset + 3],
        ]);

        // Select opcode with balanced distribution
        let opcode = select_opcode(raw, i, length, &mut opcode_histogram);

        // Extract register/immediate fields from the raw word
        let dst = ((raw >> 8) & 0x1F) as u8;
        let src_a = ((raw >> 13) & 0x1F) as u8;
        let src_b = ((raw >> 18) & 0x1F) as u8;
        let imm5 = ((raw >> 23) & 0x1F) as u8;

        // Compute flags based on opcode
        let flags = compute_flags(opcode, raw);

        instructions.push(Instruction::new(opcode, dst, src_a, src_b, imm5, flags));
    }

    // Ensure meaningful program: inject critical operations at fixed positions
    inject_guaranteed_ops(&mut instructions, seed);

    // Generate cross-lane shuffle map
    let cross_lane_offset = length * 4;
    let mut cross_lane_map = vec![0u8; LANES];
    for i in 0..LANES {
        cross_lane_map[i] = keystream[cross_lane_offset + i] % LANES as u8;
        // Ensure no lane maps to itself
        if cross_lane_map[i] == i as u8 {
            cross_lane_map[i] = (cross_lane_map[i] + 1) % LANES as u8;
        }
    }

    Program {
        instructions,
        cross_lane_map,
    }
}

/// Expands seed for program generation with domain separation.
fn expand_for_program(seed: &[u8; 32], output: &mut [u8]) {
    let blocks = output.len() / 32;
    for i in 0..blocks {
        let mut hasher = Sha256::new();
        hasher.update(b"QelloxHashV1/program-expand/v1");
        hasher.update(seed);
        hasher.update((i as u32).to_le_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        output[i * 32..(i + 1) * 32].copy_from_slice(&hash);
    }
}

/// Selects an opcode using weighted group distribution.
fn select_opcode(raw: u32, position: usize, _total: usize, histogram: &mut [u32; 24]) -> u8 {
    const GROUP_OPCODES: &[&[u8]] = &[
        &[0x01, 0x02, 0x14, 0x0B],       // Arithmetic
        &[0x03, 0x04, 0x05, 0x06, 0x15], // Bitwise
        &[0x07, 0x08, 0x16],             // Rotation
        &[0x09, 0x0A, 0x17],             // Multiplication
        &[0x0C, 0x0D, 0x0E, 0x0F],       // Bit manipulation
        &[0x10, 0x11, 0x12],             // Memory
        &[0x13],                         // Cross-lane
    ];

    const GROUP_WEIGHTS: &[u32] = &[25, 20, 15, 15, 10, 12, 3];

    // Position-based entropy + raw word for group selection
    let group_selector = raw.wrapping_add((position as u32).wrapping_mul(0x9E37_79B9));
    let total_weight: u32 = GROUP_WEIGHTS.iter().sum();
    let mut selection = group_selector % total_weight;

    let mut group_index = 0;
    for (i, &weight) in GROUP_WEIGHTS.iter().enumerate() {
        if selection < weight {
            group_index = i;
            break;
        }
        selection -= weight;
        group_index = i;
    }

    // Select specific opcode within group using secondary entropy
    let group = GROUP_OPCODES[group_index];
    let secondary = (raw >> 16).wrapping_add((position as u32).wrapping_mul(0x85EB_CA6B));
    let opcode_index = (secondary as usize) % group.len();
    let opcode = group[opcode_index];

    // Update histogram
    if (opcode as usize) < histogram.len() {
        histogram[opcode as usize] += 1;
    }

    opcode
}

/// Computes flags for an instruction based on its opcode and raw encoding.
fn compute_flags(opcode: u8, raw: u32) -> u8 {
    match opcode {
        // Memory operations: flag bit 0 = 1 for dependent addressing
        0x10..=0x12 => ((raw >> 24) & 0x0F) as u8 | 0x01,
        // Cross-lane: flag indicates mixing mode
        0x13 => ((raw >> 24) & 0x03) as u8,
        // Others: flags encode minor variants
        _ => ((raw >> 28) & 0x0F) as u8,
    }
}

/// Injects guaranteed operations (dataset load, cross-lane, scratchpad) at fixed positions.
fn inject_guaranteed_ops(instructions: &mut [Instruction], seed: &[u8; 32]) {
    let len = instructions.len();
    if len < 16 {
        return;
    }

    // Inject dataset loads at positions spread across the program
    let positions_per_pass = len / 4;
    for pass in 0..4 {
        let base = pass * positions_per_pass;
        if base + 3 < len {
            // Dataset load
            let dst = seed[pass] & 0x1F;
            let src_a = seed[pass + 4] & 0x1F;
            instructions[base] = Instruction::new(0x12, dst, src_a, 0, (pass as u8) & 0x1F, 0x01);
            // Cross-lane shuffle
            instructions[base + 2] =
                Instruction::new(0x13, dst, src_a, 0, (pass as u8) & 0x1F, 0x00);
        }
    }

    // Ensure scratchpad interaction in first pass
    let sp_dst = seed[8] & 0x1F;
    let sp_src = seed[9] & 0x1F;
    instructions[1] = Instruction::new(0x10, sp_dst, sp_src, 0, 0, 0x01);
    instructions[3] = Instruction::new(0x11, sp_src, sp_dst, 0, 4, 0x01);
}
