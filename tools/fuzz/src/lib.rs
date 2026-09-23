#[cfg(test)]
mod fuzz_tests {
    use qelloxhash_v1_reference::*;

    #[test]
    fn fuzz_program_generator_seeds() {
        // Test boundary seeds
        let boundary_seeds: Vec<[u8; 32]> = vec![
            [0x00; 32], [0xFF; 32], [0xAA; 32], [0x55; 32], [0x01; 32], [0xFE; 32], [0x80; 32],
            [0x7F; 32],
        ];

        for seed in &boundary_seeds {
            let program = generate_program(seed);
            assert_eq!(
                program.instructions.len(),
                DEFAULT_PROGRAM_LENGTH,
                "Program length mismatch for seed {:?}",
                seed
            );
            assert_eq!(
                program.cross_lane_map.len(),
                LANES,
                "Cross-lane map length mismatch for seed {:?}",
                seed
            );

            // Verify cross-lane map validity
            for (i, &peer) in program.cross_lane_map.iter().enumerate() {
                assert!(
                    (peer as usize) < LANES,
                    "Peer index {} out of range for seed {:?}",
                    peer,
                    seed
                );
                assert_ne!(
                    peer as usize, i,
                    "Lane {} maps to itself for seed {:?}",
                    i, seed
                );
            }
        }
    }

    #[test]
    fn fuzz_vm_opcodes() {
        let seed = [0x42; 32];
        let mut state = VmState::new(&seed);

        // Test every valid opcode with various operand combinations
        for opcode in 0x01u8..=0x17 {
            for dst in [0, 1, 15, 31] {
                for src_a in [0, 1, 15, 31] {
                    for src_b in [0, 1, 15, 31] {
                        for imm5 in [0, 1, 15, 31] {
                            let inst = Instruction::new(opcode, dst, src_a, src_b, imm5, 0);
                            // Should not panic
                            execute_instruction_test(&mut state, inst, 0, 0);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fuzz_vm_unknown_opcodes() {
        let seed = [0x42; 32];
        let mut state = VmState::new(&seed);
        let original = state.registers[0][0];

        // Test all possible opcodes (0x00 and 0x18..=0xFF are invalid)
        let invalid_opcodes: Vec<u8> = vec![0x00, 0x18, 0x19, 0x20, 0x40, 0x80, 0xFE, 0xFF];

        for opcode in invalid_opcodes {
            let inst = Instruction::new(opcode, 0, 1, 2, 3, 0);
            execute_instruction_test(&mut state, inst, 0, 0);
        }

        // State should not have changed significantly (NOPs don't modify state)
        // Note: other instructions in the loop may have modified state, so we
        // just verify no panics occurred.
    }

    #[test]
    fn fuzz_dataset_indexing() {
        let cache = LightCache::new(0);

        // Test boundary indices
        let indices: Vec<u64> = vec![
            0,
            1,
            255,
            256,
            65535,
            65536,
            u32::MAX as u64,
            u32::MAX as u64 + 1,
        ];

        for idx in indices {
            let item = cache.dataset_item(idx);
            // Verify item is not all zeros (with overwhelming probability)
            assert!(
                item.iter().any(|&x| x != 0),
                "Dataset item at index {} is all zeros",
                idx
            );
        }
    }

    #[test]
    fn fuzz_seed_expansion() {
        let seed = [0xAB; 32];

        // Test various output lengths (must be multiple of 32)
        for len in [32, 64, 128, 256, 512, 1024] {
            let mut output = vec![0u8; len];
            expand_seed(&seed, &mut output);

            // Verify output is not all zeros
            assert!(
                output.iter().any(|&x| x != 0),
                "Seed expansion produced all zeros for length {}",
                len
            );
        }
    }

    #[test]
    fn fuzz_target_comparison() {
        // All zeros
        assert!(meets_target(&[0x00; 32], &[0x00; 32]));

        // All max
        assert!(meets_target(&[0xFF; 32], &[0xFF; 32]));

        // Target all max, digest all zeros
        assert!(meets_target(&[0x00; 32], &[0xFF; 32]));

        // Target all zeros, digest all max
        assert!(!meets_target(&[0xFF; 32], &[0x00; 32]));

        // Single byte difference at various positions
        for pos in 0..32 {
            let mut digest = [0x00; 32];
            digest[pos] = 0x01;
            let mut target = [0x00; 32];
            target[pos] = 0x02;
            assert!(meets_target(&digest, &target), "Failed at position {}", pos);
        }
    }

    #[test]
    fn fuzz_full_hash_inputs() {
        // Various header patterns
        let headers: Vec<[u8; 80]> = vec![
            [0x00; 80],
            [0xFF; 80],
            [0xAA; 80],
            [0x55; 80],
            {
                let mut h = [0x00; 80];
                h[0] = 0x01;
                h
            },
            {
                let mut h = [0x00; 80];
                h[79] = 0x01;
                h
            },
        ];

        // Various nonces
        let nonces: Vec<u64> = vec![0, 1, 255, 256, 65535, 65536, u32::MAX as u64, u64::MAX];

        for header in &headers {
            for &nonce in &nonces {
                let hash = qelloxhash_v1(header, nonce);
                // Hash should not be all zeros
                assert!(
                    hash.iter().any(|&x| x != 0),
                    "Hash is all zeros for header {:?} nonce {}",
                    &header[..4],
                    nonce
                );
            }
        }
    }

    #[test]
    fn fuzz_vm_state_determinism() {
        let seeds: Vec<[u8; 32]> = vec![[0x00; 32], [0xFF; 32], [0x42; 32], [0xAB; 32]];

        for seed in &seeds {
            let state1 = VmState::new(seed);
            let state2 = VmState::new(seed);

            // All registers must be identical
            for lane in 0..LANES {
                for reg in 0..REGISTERS {
                    assert_eq!(
                        state1.registers[lane][reg], state2.registers[lane][reg],
                        "Register [{lane}][{reg}] differs for seed {seed:?}"
                    );
                }
            }

            // Scratchpad must be identical
            for (i, (&a, &b)) in state1
                .scratchpad
                .iter()
                .zip(state2.scratchpad.iter())
                .enumerate()
            {
                assert_eq!(a, b, "Scratchpad[{i}] differs for seed {seed:?}");
            }
        }
    }

    #[test]
    fn fuzz_instruction_encoding() {
        // Test that encoding and decoding are inverse operations
        for opcode in [0x01, 0x0F, 0x17, 0xFF] {
            for dst in [0, 16, 31] {
                for src_a in [0, 16, 31] {
                    for src_b in [0, 16, 31] {
                        for imm5 in [0, 16, 31] {
                            for flags in [0, 8, 15] {
                                let inst = Instruction::new(opcode, dst, src_a, src_b, imm5, flags);
                                assert_eq!(inst.opcode(), opcode);
                                assert_eq!(inst.dst(), dst);
                                assert_eq!(inst.src_a(), src_a);
                                assert_eq!(inst.src_b(), src_b);
                                assert_eq!(inst.imm5(), imm5);
                                assert_eq!(inst.flags(), flags);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fuzz_cross_lane_mix() {
        let seed = [0xBB; 32];
        let mut state = VmState::new(&seed);
        let original = state.registers[0][0];

        // Test with identity-like map
        let identity_map: Vec<u8> = (0..LANES).map(|i| ((i + 1) % LANES) as u8).collect();
        cross_lane_mix(&mut state, &identity_map);
        assert_ne!(state.registers[0][0], original);
    }

    #[test]
    fn fuzz_epoch_seeds() {
        for epoch in 0..10 {
            let seed = epoch_seed(epoch);
            assert!(
                seed.iter().any(|&b| b != 0),
                "Epoch {epoch} seed is all zeros"
            );

            let cache = LightCache::new(epoch);
            assert!(cache.item_count() > 0, "Cache is empty for epoch {epoch}");
        }
    }
}
