use sha2::Sha256;

use crate::program::opcode;
use crate::types::{Instruction, Program, VmState};
use crate::{DATASET_READS_PER_PASS, LANES, REGISTERS, SCRATCHPAD_WORDS, SUBGROUP_SIZE};

impl VmState {
    /// Creates a new VM state initialized from the mining seed.
    #[must_use]
    pub fn new(seed: &[u8; 32]) -> Self {
        let mut registers = [[0u32; REGISTERS]; LANES];

        // Initialize registers from seed expansion
        // Each lane gets a unique initialization derived from the seed
        for (lane, lane_regs) in registers.iter_mut().enumerate() {
            for (reg, reg_val) in lane_regs.iter_mut().enumerate() {
                let mut hasher = Sha256::new();
                use sha2::Digest;
                hasher.update(b"QelloxHashV1/reg-init/v1");
                hasher.update(seed);
                hasher.update((lane as u32).to_le_bytes());
                hasher.update((reg as u32).to_le_bytes());
                let hash: [u8; 32] = hasher.finalize().into();
                *reg_val = u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]);
            }
        }

        // Initialize scratchpad from seed
        let scratchpad_words = SCRATCHPAD_WORDS;
        let mut scratchpad = vec![0u32; scratchpad_words];
        let expand_blocks = scratchpad_words.div_ceil(8);
        for i in 0..expand_blocks {
            let mut hasher = Sha256::new();
            use sha2::Digest;
            hasher.update(b"QelloxHashV1/scratch-init/v1");
            hasher.update(seed);
            hasher.update((i as u32).to_le_bytes());
            let hash: [u8; 32] = hasher.finalize().into();
            for j in 0..8 {
                let idx = i * 8 + j;
                if idx < scratchpad_words {
                    scratchpad[idx] = u32::from_le_bytes([
                        hash[j * 4],
                        hash[j * 4 + 1],
                        hash[j * 4 + 2],
                        hash[j * 4 + 3],
                    ]);
                }
            }
        }

        Self {
            registers,
            scratchpad,
        }
    }
}

/// Executes one pass: instructions, cross-lane mixing, dataset reads.
pub fn execute_pass(state: &mut VmState, program: &Program, _epoch: u32, pass: usize) {
    let instructions = &program.instructions;
    let cross_lane_map = &program.cross_lane_map;

    // Phase 1: Execute instructions
    for (ip, instruction) in instructions.iter().enumerate() {
        execute_instruction(state, *instruction, ip, pass);
    }

    // Phase 2: Cross-lane mixing
    cross_lane_mix(state, cross_lane_map);

    // Phase 3: Dataset reads with dependency chains
    dataset_reads(state, pass);
}

/// Executes a single instruction across all lanes (public test entry point).
pub fn execute_instruction_test(state: &mut VmState, inst: Instruction, ip: usize, pass: usize) {
    execute_instruction(state, inst, ip, pass);
}

/// Executes a single instruction across all lanes.
fn execute_instruction(state: &mut VmState, inst: Instruction, _ip: usize, _pass: usize) {
    let opcode = inst.opcode();
    let dst = inst.dst() as usize;
    let src_a = inst.src_a() as usize;
    let src_b = inst.src_b() as usize;
    let imm5 = inst.imm5() as u32;
    let _flags = inst.flags();

    // Bounds check (should never fail with 5-bit fields and 32 registers)
    debug_assert!(dst < REGISTERS && src_a < REGISTERS && src_b < REGISTERS);

    match opcode {
        opcode::ADD32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] =
                    state.registers[lane][src_a].wrapping_add(state.registers[lane][src_b]);
            }
        }
        opcode::SUB32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] =
                    state.registers[lane][src_a].wrapping_sub(state.registers[lane][src_b]);
            }
        }
        opcode::XOR32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] =
                    state.registers[lane][src_a] ^ state.registers[lane][src_b];
            }
        }
        opcode::AND32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] =
                    state.registers[lane][src_a] & state.registers[lane][src_b];
            }
        }
        opcode::OR32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] =
                    state.registers[lane][src_a] | state.registers[lane][src_b];
            }
        }
        opcode::NOT32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] = !state.registers[lane][src_a];
            }
        }
        opcode::ROTL32 => {
            for lane in 0..LANES {
                let shift = state.registers[lane][src_b] & 31;
                state.registers[lane][dst] = state.registers[lane][src_a].rotate_left(shift);
            }
        }
        opcode::ROTR32 => {
            for lane in 0..LANES {
                let shift = state.registers[lane][src_b] & 31;
                state.registers[lane][dst] = state.registers[lane][src_a].rotate_right(shift);
            }
        }
        opcode::MUL_LO32 => {
            for lane in 0..LANES {
                let a = state.registers[lane][src_a] as u64;
                let b = state.registers[lane][src_b] as u64;
                state.registers[lane][dst] = (a.wrapping_mul(b)) as u32;
            }
        }
        opcode::MUL_HI32 => {
            for lane in 0..LANES {
                let a = state.registers[lane][src_a] as u64;
                let b = state.registers[lane][src_b] as u64;
                state.registers[lane][dst] = (a.wrapping_mul(b) >> 32) as u32;
            }
        }
        opcode::MADD32 => {
            // dst = src_a.wrapping_add(src_b.wrapping_mul(imm5))
            for lane in 0..LANES {
                let product = state.registers[lane][src_b].wrapping_mul(imm5);
                state.registers[lane][dst] = state.registers[lane][src_a].wrapping_add(product);
            }
        }
        opcode::POPCNT32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] = state.registers[lane][src_a].count_ones();
            }
        }
        opcode::CLZ32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] = state.registers[lane][src_a].leading_zeros();
            }
        }
        opcode::BITSELECT => {
            // dst = (src_a & src_c) | (src_b & !src_c) where src_c = register[imm5]
            let src_c = (imm5 & 0x1F) as usize;
            for lane in 0..LANES {
                let c = state.registers[lane][src_c];
                state.registers[lane][dst] =
                    (state.registers[lane][src_a] & c) | (state.registers[lane][src_b] & !c);
            }
        }
        opcode::BYTE_PERMUTE => {
            // Permute bytes of src_a using src_b as control
            // Each byte of src_b selects which byte of src_a goes to that position
            for lane in 0..LANES {
                let a = state.registers[lane][src_a].to_le_bytes();
                let b = state.registers[lane][src_b].to_le_bytes();
                let mut result = [0u8; 4];
                for i in 0..4 {
                    result[i] = a[(b[i] & 0x03) as usize];
                }
                state.registers[lane][dst] = u32::from_le_bytes(result);
            }
        }
        opcode::SCRATCH_LOAD => {
            let base_offset = imm5 as usize;
            for lane in 0..LANES {
                let index = (state.registers[lane][src_a] as usize).wrapping_add(base_offset);
                state.registers[lane][dst] = state.scratchpad[index % SCRATCHPAD_WORDS];
            }
        }
        opcode::SCRATCH_STORE => {
            let base_offset = imm5 as usize;
            for lane in 0..LANES {
                let index = (state.registers[lane][src_b] as usize).wrapping_add(base_offset);
                state.scratchpad[index % SCRATCHPAD_WORDS] ^= state.registers[lane][src_a];
            }
        }
        opcode::DATASET_LOAD => {
            // Dataset load - uses address dependent on register state
            // The actual dataset fetch is deferred to dataset_reads() phase
            // Here we compute the index and store it in dst register
            for lane in 0..LANES {
                let addr = state.registers[lane][src_a]
                    .wrapping_add(state.registers[lane][src_b])
                    .wrapping_add(imm5);
                state.registers[lane][dst] = addr;
            }
        }
        opcode::LANE_SHUFFLE => {
            // Cross-lane register read
            for lane in 0..LANES {
                let peer = (lane + 1 + (imm5 as usize)) % LANES;
                state.registers[lane][dst] = state.registers[peer][src_a];
            }
        }
        opcode::ADD_IMM32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] = state.registers[lane][src_a].wrapping_add(imm5);
            }
        }
        opcode::XOR_IMM32 => {
            for lane in 0..LANES {
                state.registers[lane][dst] = state.registers[lane][src_a] ^ imm5;
            }
        }
        opcode::ROTL_IMM32 => {
            let shift = imm5 & 31;
            for lane in 0..LANES {
                state.registers[lane][dst] = state.registers[lane][src_a].rotate_left(shift);
            }
        }
        opcode::MUL_ROT32 => {
            let shift = imm5 & 31;
            for lane in 0..LANES {
                let a = state.registers[lane][src_a] as u64;
                let b = state.registers[lane][src_b] as u64;
                let product = (a.wrapping_mul(b)) as u32;
                state.registers[lane][dst] = product.rotate_left(shift);
            }
        }
        _ => {} // NOP for unknown opcodes
    }
}

/// Cross-lane mixing: peer exchange, XOR-rotate, subgroup butterfly.
pub fn cross_lane_mix(state: &mut VmState, cross_lane_map: &[u8]) {
    // Phase 1: Gather peer register values
    let mut peer_values = [[0u32; REGISTERS]; LANES];
    for lane in 0..LANES {
        let peer = cross_lane_map[lane] as usize;
        peer_values[lane].copy_from_slice(&state.registers[peer]);
    }

    // Phase 2: XOR-rotate mix with peer values
    for (lane, lane_regs) in state.registers.iter_mut().enumerate() {
        for (reg, reg_val) in lane_regs.iter_mut().enumerate() {
            let mixed = *reg_val ^ peer_values[lane][reg].rotate_left((reg as u32) & 31);
            *reg_val = mixed;
        }
    }

    // Phase 3: Subgroup butterfly
    for subgroup_start in (0..LANES).step_by(SUBGROUP_SIZE) {
        let subgroup_end = (subgroup_start + SUBGROUP_SIZE).min(LANES);
        let subgroup_size = subgroup_end - subgroup_start;

        // Logarithmic butterfly within subgroup
        let mut stride = 1;
        while stride < subgroup_size {
            for i in (subgroup_start..subgroup_end).step_by(stride * 2) {
                let j = i + stride;
                if j < subgroup_end {
                    for reg in 0..REGISTERS {
                        let a = state.registers[i][reg];
                        let b = state.registers[j][reg];
                        state.registers[i][reg] = a.wrapping_add(b).rotate_left(7);
                        state.registers[j][reg] = b.wrapping_sub(a).rotate_left(13);
                    }
                }
            }
            stride *= 2;
        }
    }
}

/// Dataset reads with serial dependency chains.
fn dataset_reads(state: &mut VmState, pass: usize) {
    for lane in 0..LANES {
        let mut chain_value = state.registers[lane][0]
            .wrapping_add(state.registers[lane][4])
            .wrapping_add(pass as u32 * 0x0100_0193);

        for read_idx in 0..DATASET_READS_PER_PASS {
            let dataset_index = chain_value
                .wrapping_mul(0x0100_0193) // FNV prime
                .wrapping_add(0x811c_9dc5); // FNV offset basis
            let scratchpad_index = (dataset_index as usize) % SCRATCHPAD_WORDS;

            let read_value = state.scratchpad[scratchpad_index];

            let dst_reg = (read_idx * 4 + lane + pass) % REGISTERS;
            state.registers[lane][dst_reg] = state.registers[lane][dst_reg]
                .wrapping_add(read_value)
                .rotate_left((read_idx as u32 + 3) & 31);

            // Dependent: next address depends on this read's result
            chain_value = read_value
                .wrapping_add(state.registers[lane][(read_idx + 1) % REGISTERS])
                .wrapping_mul(0x0100_0193);

            // Write-back creates read-write dependencies
            state.scratchpad[scratchpad_index] ^= state.registers[lane][(read_idx + 2) % REGISTERS];
        }
    }
}

#[must_use]
pub fn dependency_depth_per_pass() -> usize {
    DATASET_READS_PER_PASS
}

#[must_use]
pub fn total_dependency_depth() -> usize {
    DATASET_READS_PER_PASS * crate::EXECUTION_PASSES
}
