use qelloxhash_v1_reference::*;
use sha2::{Digest, Sha256};

fn main() {
    let header = [0x00u8; 80];
    let nonce: u64 = 0;
    let epoch: u32 = 0;

    println!("=== QelloxHashV1 Single-Nonce Walkthrough ===");
    println!("Header: all zeros (80 bytes)");
    println!("Nonce: {}", nonce);
    println!("Epoch: {}", epoch);
    println!();

    // Step 1: Mining seed
    let seed = {
        let mut hasher = Sha256::new();
        hasher.update(b"QelloxHashV1/seed/v1");
        hasher.update(&header);
        hasher.update(nonce.to_le_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        hash
    };
    println!("Step 1: Mining Seed");
    println!("  SHA-256(\"QelloxHashV1/seed/v1\" || header[80] || le64(0))");
    println!("  = {}", hex::encode(&seed));
    println!();

    // Step 2: Program generation
    let program = generate_program(&seed);
    println!("Step 2: Program Generation");
    println!("  Keystream: 15 SHA-256 blocks (480 bytes)");
    println!("  Instructions: {}", program.instructions.len());
    println!("  Cross-lane map[0..8]: {:?}", &program.cross_lane_map[..8]);
    println!();
    println!("  First 8 instructions (of 112):");
    for (i, inst) in program.instructions.iter().take(8).enumerate() {
        let name = match inst.opcode() {
            0x01 => "ADD32",
            0x02 => "SUB32",
            0x03 => "XOR32",
            0x04 => "AND32",
            0x05 => "OR32",
            0x06 => "NOT32",
            0x07 => "ROTL32",
            0x08 => "ROTR32",
            0x09 => "MUL_LO32",
            0x0A => "MUL_HI32",
            0x0B => "MADD32",
            0x0C => "POPCNT32",
            0x0D => "CLZ32",
            0x0E => "BITSELECT",
            0x0F => "BYTE_PERMUTE",
            0x10 => "SCRATCH_LOAD",
            0x11 => "SCRATCH_STORE",
            0x12 => "DATASET_LOAD",
            0x13 => "LANE_SHUFFLE",
            0x14 => "ADD_IMM32",
            0x15 => "XOR_IMM32",
            0x16 => "ROTL_IMM32",
            0x17 => "MUL_ROT32",
            _ => "NOP",
        };
        println!(
            "    [{}] {} opcode={:#04x} dst=R{} src_a=R{} src_b=R{} imm5={}",
            i,
            name,
            inst.opcode(),
            inst.dst(),
            inst.src_a(),
            inst.src_b(),
            inst.imm5()
        );
    }
    println!();
    println!("  Guaranteed ops injected:");
    println!("    [0] DATASET_LOAD  (pass 0 boundary)");
    println!("    [1] SCRATCH_LOAD  (pass 0)");
    println!("    [2] LANE_SHUFFLE  (pass 0 boundary)");
    println!("    [3] SCRATCH_STORE (pass 0)");
    println!("    [28] DATASET_LOAD (pass 1 boundary)");
    println!("    [30] LANE_SHUFFLE (pass 1 boundary)");
    println!("    [56] DATASET_LOAD (pass 2 boundary)");
    println!("    [58] LANE_SHUFFLE (pass 2 boundary)");
    println!("    [84] DATASET_LOAD (pass 3 boundary)");
    println!("    [86] LANE_SHUFFLE (pass 3 boundary)");
    println!();

    // Step 3: Register initialization
    let state = VmState::new(&seed);
    println!("Step 3: State Initialization (1024 + 1536 SHA-256)");
    println!(
        "  Lane 0, R0-R3:  [{}, {}, {}, {}]",
        state.registers[0][0], state.registers[0][1], state.registers[0][2], state.registers[0][3]
    );
    println!("  Lane 0, R0 (hex): {:#010x}", state.registers[0][0]);
    println!(
        "  Lane 1, R0-R3:  [{}, {}, {}, {}]",
        state.registers[1][0], state.registers[1][1], state.registers[1][2], state.registers[1][3]
    );
    println!(
        "  Lane 31, R0-R3: [{}, {}, {}, {}]",
        state.registers[31][0],
        state.registers[31][1],
        state.registers[31][2],
        state.registers[31][3]
    );
    println!(
        "  Scratchpad[0..4]: [{}, {}, {}, {}]",
        state.scratchpad[0], state.scratchpad[1], state.scratchpad[2], state.scratchpad[3]
    );
    println!();

    // Step 4: Full pipeline using library
    println!("Step 4: Full Pipeline (using library qelloxhash_v1_with_epoch)");
    let pow_digest = qelloxhash_v1_with_epoch(&header, nonce, epoch);
    println!("  PoW digest: {}", hex::encode(&pow_digest));
    println!();

    // Verify
    let verify = qelloxhash_v1(&header, nonce);
    println!("Verification:");
    println!("  qelloxhash_v1(header, 0) = {}", hex::encode(&verify));
    println!("  Match: {}", pow_digest == verify);
    println!();

    // Show some intermediate data
    println!("=== Key Intermediate Values ===");
    println!("  Mining seed:     {}", hex::encode(&seed));
    println!("  PoW digest:      {}", hex::encode(&pow_digest));
    println!();

    // Show what happens with a different nonce
    let pow1 = qelloxhash_v1(&header, 1);
    println!("  Nonce 1 PoW:     {}", hex::encode(&pow1));
    println!("  Different from nonce 0: {}", pow_digest != pow1);

    // Show what happens with a different header
    let header_ff = [0xFFu8; 80];
    let pow_ff = qelloxhash_v1(&header_ff, 0);
    println!("  0xFF header PoW: {}", hex::encode(&pow_ff));
    println!("  Different from 0x00 header: {}", pow_digest != pow_ff);
}
