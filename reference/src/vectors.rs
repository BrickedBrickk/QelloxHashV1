#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::vm;
    use sha2::{Digest, Sha256};

    #[test]
    fn test_epoch_from_height() {
        assert_eq!(epoch_from_height(0), 0);
        assert_eq!(epoch_from_height(74_999), 0);
        assert_eq!(epoch_from_height(75_000), 1);
        assert_eq!(epoch_from_height(149_999), 1);
        assert_eq!(epoch_from_height(150_000), 2);
    }

    #[test]
    fn test_epoch_seed_deterministic() {
        let s0 = epoch_seed(0);
        let s0_again = epoch_seed(0);
        let s1 = epoch_seed(1);
        assert_eq!(s0, s0_again);
        assert_ne!(s0, s1);
    }

    #[test]
    fn test_expand_seed_deterministic() {
        let seed = [0xAB_u8; 32];
        let mut out1 = [0u8; 128];
        let mut out2 = [0u8; 128];
        expand_seed(&seed, &mut out1);
        expand_seed(&seed, &mut out2);
        assert_eq!(out1, out2);
    }

    #[test]
    fn test_expand_seed_different_seeds() {
        let seed1 = [0x00_u8; 32];
        let seed2 = [0xFF_u8; 32];
        let mut out1 = [0u8; 64];
        let mut out2 = [0u8; 64];
        expand_seed(&seed1, &mut out1);
        expand_seed(&seed2, &mut out2);
        assert_ne!(out1, out2);
    }

    #[test]
    fn test_instruction_encoding_roundtrip() {
        let inst = Instruction::new(0x12, 15, 7, 3, 17, 0x0A);
        assert_eq!(inst.opcode(), 0x12);
        assert_eq!(inst.dst(), 15);
        assert_eq!(inst.src_a(), 7);
        assert_eq!(inst.src_b(), 3);
        assert_eq!(inst.imm5(), 17);
        assert_eq!(inst.flags(), 0x0A);
    }

    #[test]
    fn test_instruction_encoding_bounds() {
        // Test maximum field values
        let inst = Instruction::new(0xFF, 31, 31, 31, 31, 0x0F);
        assert_eq!(inst.opcode(), 0xFF);
        assert_eq!(inst.dst(), 31);
        assert_eq!(inst.src_a(), 31);
        assert_eq!(inst.src_b(), 31);
        assert_eq!(inst.imm5(), 31);
        assert_eq!(inst.flags(), 0x0F);
    }

    #[test]
    fn test_program_generation_deterministic() {
        let seed = [0x42_u8; 32];
        let prog1 = generate_program(&seed);
        let prog2 = generate_program(&seed);
        assert_eq!(prog1.instructions.len(), prog2.instructions.len());
        for (a, b) in prog1.instructions.iter().zip(prog2.instructions.iter()) {
            assert_eq!(a, b);
        }
        assert_eq!(prog1.cross_lane_map, prog2.cross_lane_map);
    }

    #[test]
    fn test_program_generation_different_seeds() {
        let seed1 = [0x00_u8; 32];
        let seed2 = [0xFF_u8; 32];
        let prog1 = generate_program(&seed1);
        let prog2 = generate_program(&seed2);
        // Programs should differ (with overwhelming probability)
        let mut any_different = false;
        for (a, b) in prog1.instructions.iter().zip(prog2.instructions.iter()) {
            if a != b {
                any_different = true;
                break;
            }
        }
        assert!(any_different, "Programs from different seeds should differ");
    }

    #[test]
    fn test_program_length() {
        let seed = [0x01_u8; 32];
        let prog = generate_program(&seed);
        assert_eq!(prog.instructions.len(), DEFAULT_PROGRAM_LENGTH);
    }

    #[test]
    fn test_cross_lane_map_valid() {
        let seed = [0x55_u8; 32];
        let prog = generate_program(&seed);
        assert_eq!(prog.cross_lane_map.len(), LANES);
        for (i, &peer) in prog.cross_lane_map.iter().enumerate() {
            assert!((peer as usize) < LANES, "Peer index out of range");
            assert_ne!(peer as usize, i, "Lane {} maps to itself", i);
        }
    }

    #[test]
    fn test_vm_state_deterministic() {
        let seed = [0xAA_u8; 32];
        let state1 = VmState::new(&seed);
        let state2 = VmState::new(&seed);
        assert_eq!(state1.registers, state2.registers);
        assert_eq!(state1.scratchpad, state2.scratchpad);
    }

    #[test]
    fn test_vm_state_different_seeds() {
        let seed1 = [0x00_u8; 32];
        let seed2 = [0xFF_u8; 32];
        let state1 = VmState::new(&seed1);
        let state2 = VmState::new(&seed2);
        assert_ne!(state1.registers[0][0], state2.registers[0][0]);
    }

    #[test]
    fn test_execute_pass_modifies_state() {
        let seed = [0x42_u8; 32];
        let prog = generate_program(&seed);
        let mut state = VmState::new(&seed);
        let original_reg = state.registers[0][0];
        execute_pass(&mut state, &prog, 0, 0);
        // State should have changed
        assert_ne!(state.registers[0][0], original_reg);
    }

    #[test]
    fn test_reduce_state_deterministic() {
        let seed = [0x77_u8; 32];
        let prog = generate_program(&seed);
        let mut state1 = VmState::new(&seed);
        let mut state2 = VmState::new(&seed);
        for pass in 0..EXECUTION_PASSES {
            execute_pass(&mut state1, &prog, 0, pass);
            execute_pass(&mut state2, &prog, 0, pass);
        }
        let digest1 = reduce_state(&state1);
        let digest2 = reduce_state(&state2);
        assert_eq!(digest1, digest2);
    }

    #[test]
    fn test_final_digest_deterministic() {
        let work = [0x12_u8; 32];
        let d1 = final_digest(&work);
        let d2 = final_digest(&work);
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_final_digest_domain_separation() {
        let work = [0x12_u8; 32];
        let d = final_digest(&work);
        // Verify it's not just a raw SHA-256 of the work digest
        let raw: [u8; 32] = Sha256::digest(&work).into();
        assert_ne!(d, raw, "Final digest must use domain separation");
    }

    #[test]
    fn test_full_hash_deterministic() {
        let header = [0x00_u8; 80];
        let nonce = 0u64;
        let h1 = qelloxhash_v1(&header, nonce);
        let h2 = qelloxhash_v1(&header, nonce);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_full_hash_different_nonce() {
        let header = [0x00_u8; 80];
        let h1 = qelloxhash_v1(&header, 0);
        let h2 = qelloxhash_v1(&header, 1);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_full_hash_different_header() {
        let header1 = [0x00_u8; 80];
        let mut header2 = [0x00_u8; 80];
        header2[0] = 0x01;
        let h1 = qelloxhash_v1(&header1, 0);
        let h2 = qelloxhash_v1(&header2, 0);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_meets_target_exact() {
        let digest = [0x00_u8; 32];
        let target = [0x00_u8; 32];
        assert!(meets_target(&digest, &target));
    }

    #[test]
    fn test_meets_target_less() {
        let mut digest = [0x00_u8; 32];
        digest[31] = 0x01;
        let mut target = [0x00_u8; 32];
        target[31] = 0x02;
        assert!(meets_target(&digest, &target));
    }

    #[test]
    fn test_meets_target_greater() {
        let mut digest = [0x00_u8; 32];
        digest[31] = 0x03;
        let mut target = [0x00_u8; 32];
        target[31] = 0x02;
        assert!(!meets_target(&digest, &target));
    }

    #[test]
    fn test_meets_target_equal() {
        let target = [0xAB_u8; 32];
        assert!(meets_target(&target, &target));
    }

    #[test]
    fn test_verify_pow_alias() {
        let digest = [0x00_u8; 32];
        let target = [0xFF_u8; 32];
        assert!(verify_pow(&digest, &target));
    }

    #[test]
    fn test_light_cache_deterministic() {
        let cache1 = LightCache::new(0);
        let cache2 = LightCache::new(0);
        assert_eq!(cache1.item_count(), cache2.item_count());
        for i in 0..cache1.item_count().min(100) {
            assert_eq!(cache1.cache_item(i), cache2.cache_item(i));
        }
    }

    #[test]
    fn test_light_cache_different_epochs() {
        let cache0 = LightCache::new(0);
        let cache1 = LightCache::new(1);
        // Cache sizes should differ
        assert_ne!(cache0.item_count(), cache1.item_count());
    }

    #[test]
    fn test_dataset_item_deterministic() {
        let cache = LightCache::new(0);
        let item1 = cache.dataset_item(0);
        let item2 = cache.dataset_item(0);
        assert_eq!(item1, item2);
    }

    #[test]
    fn test_dataset_item_different_indices() {
        let cache = LightCache::new(0);
        let item0 = cache.dataset_item(0);
        let item1 = cache.dataset_item(1);
        assert_ne!(item0, item1);
    }

    #[test]
    fn test_cache_item_count_growth() {
        let count0 = cache_item_count(0);
        let count1 = cache_item_count(1);
        assert!(count1 > count0, "Cache should grow per epoch");
    }

    #[test]
    fn test_dataset_item_count_growth() {
        let count0 = dataset_item_count(0);
        let count1 = dataset_item_count(1);
        assert!(count1 > count0, "Dataset should grow per epoch");
    }

    #[test]
    fn test_dependency_depth() {
        assert_eq!(dependency_depth_per_pass(), DATASET_READS_PER_PASS);
        assert_eq!(
            total_dependency_depth(),
            DATASET_READS_PER_PASS * EXECUTION_PASSES
        );
    }

    #[test]
    fn test_all_opcodes_no_panic() {
        // Test that every opcode executes without panicking
        for op in 0x01u8..=0x17 {
            let inst = Instruction::new(op, 0, 1, 2, 3, 0);
            let seed = [0x42_u8; 32];
            let mut state = VmState::new(&seed);
            // This should not panic for any opcode
            vm::execute_instruction_test(&mut state, inst, 0, 0);
        }
    }

    #[test]
    fn test_scratchpad_interaction() {
        let seed = [0x99_u8; 32];
        let mut state = VmState::new(&seed);

        // Record all scratchpad values before
        let before: Vec<u32> = state.scratchpad.clone();

        // First, set register 1 to a non-zero value via XOR_IMM
        let prep = Instruction::new(0x15, 1, 0, 0, 5, 0); // XOR_IMM32 dst=1, src_a=0, imm5=5
        vm::execute_instruction_test(&mut state, prep, 0, 0);

        // Execute a scratchpad store using register 1 as source
        let inst = Instruction::new(0x11, 0, 1, 0, 0, 0x01); // SCRATCH_STORE
        vm::execute_instruction_test(&mut state, inst, 0, 0);

        // At least one scratchpad word should have changed
        let changed = state
            .scratchpad
            .iter()
            .zip(before.iter())
            .any(|(a, b)| a != b);
        assert!(changed, "Scratchpad should have changed after store");
    }

    #[test]
    fn test_cross_lane_mix_modifies_state() {
        let seed = [0xBB_u8; 32];
        let mut state = VmState::new(&seed);
        let prog = generate_program(&seed);
        let original = state.registers[0][0];

        vm::cross_lane_mix(&mut state, &prog.cross_lane_map);
        assert_ne!(state.registers[0][0], original);
    }
}
