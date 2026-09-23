use serde::{Deserialize, Serialize};

use crate::architecture::create_architecture_database;
use crate::candidate::{baseline_v1, CandidateProfile, InstructionWeights};
use crate::scoring::{score_candidate, CandidateScore, ScoringWeights};

/// Search result containing all evaluated candidates.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub baseline_score: CandidateScore,
    pub candidates: Vec<ScoredCandidate>,
    pub finalists: Vec<ScoredCandidate>,
    pub recommended: Option<ScoredCandidate>,
}

/// A candidate with its score.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoredCandidate {
    pub profile: CandidateProfile,
    pub score: CandidateScore,
}

/// Runs the full parameter search.
pub fn run_search() -> SearchResult {
    let architectures = create_architecture_database();
    let weights = ScoringWeights::default();

    // Score baseline
    let baseline = baseline_v1();
    let baseline_score = score_candidate(&baseline, &architectures, &weights);

    // Generate candidate variants
    let candidates = generate_candidates();

    // Score all candidates
    let mut scored: Vec<ScoredCandidate> = candidates
        .into_iter()
        .map(|profile| {
            let score = score_candidate(&profile, &architectures, &weights);
            ScoredCandidate { profile, score }
        })
        .collect();

    // Sort by total score (descending)
    scored.sort_by(|a, b| {
        b.score
            .total_score
            .partial_cmp(&a.score.total_score)
            .unwrap()
    });

    // Select finalists (top 3 non-rejected)
    let finalists: Vec<ScoredCandidate> = scored
        .iter()
        .filter(|c| !c.score.rejected)
        .take(3)
        .cloned()
        .collect();

    // Select recommended candidate
    let recommended = finalists.first().cloned();

    SearchResult {
        baseline_score,
        candidates: scored,
        finalists,
        recommended,
    }
}

/// Generates a diverse set of candidate profiles for evaluation.
fn generate_candidates() -> Vec<CandidateProfile> {
    let mut candidates = Vec::new();

    // === REGISTER PRESSURE STUDY ===
    for regs in [24, 32, 40, 48, 56] {
        candidates.push(CandidateProfile {
            name: format!("regs-{}", regs),
            lanes: 32,
            registers: regs,
            program_length: 112,
            execution_passes: 4,
            scratchpad_bytes: 49152,
            dataset_reads_per_pass: 8,
            dependent_reads_per_pass: 8,
            cross_lane_rounds: 2,
            subgroup_size: 8,
            alu_work_between_reads: 0,
            scratch_ops_per_pass: 4,
            dataset_size_gib: 2.0,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === PROGRAM SIZE STUDY ===
    for prog_len in [64, 80, 96, 112, 128, 160] {
        candidates.push(CandidateProfile {
            name: format!("prog-{}", prog_len),
            lanes: 32,
            registers: 32,
            program_length: prog_len,
            execution_passes: 4,
            scratchpad_bytes: 49152,
            dataset_reads_per_pass: 8,
            dependent_reads_per_pass: 8,
            cross_lane_rounds: 2,
            subgroup_size: 8,
            alu_work_between_reads: 0,
            scratch_ops_per_pass: 4,
            dataset_size_gib: 2.0,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === EXECUTION PASSES STUDY ===
    for passes in [2, 3, 4, 5, 6, 8] {
        candidates.push(CandidateProfile {
            name: format!("passes-{}", passes),
            lanes: 32,
            registers: 32,
            program_length: 112,
            execution_passes: passes,
            scratchpad_bytes: 49152,
            dataset_reads_per_pass: 8,
            dependent_reads_per_pass: 8,
            cross_lane_rounds: 2,
            subgroup_size: 8,
            alu_work_between_reads: 0,
            scratch_ops_per_pass: 4,
            dataset_size_gib: 2.0,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === SCRATCHPAD SIZE STUDY ===
    for scratch_kb in [16, 24, 32, 48, 64] {
        candidates.push(CandidateProfile {
            name: format!("scratch-{}k", scratch_kb),
            lanes: 32,
            registers: 32,
            program_length: 112,
            execution_passes: 4,
            scratchpad_bytes: scratch_kb * 1024,
            dataset_reads_per_pass: 8,
            dependent_reads_per_pass: 8,
            cross_lane_rounds: 2,
            subgroup_size: 8,
            alu_work_between_reads: 0,
            scratch_ops_per_pass: 4,
            dataset_size_gib: 2.0,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === DEPENDENT READS STUDY ===
    for dep_reads in [2, 4, 6, 8, 12, 16] {
        candidates.push(CandidateProfile {
            name: format!("depreads-{}", dep_reads),
            lanes: 32,
            registers: 32,
            program_length: 112,
            execution_passes: 4,
            scratchpad_bytes: 49152,
            dataset_reads_per_pass: dep_reads,
            dependent_reads_per_pass: dep_reads,
            cross_lane_rounds: 2,
            subgroup_size: 8,
            alu_work_between_reads: 0,
            scratch_ops_per_pass: 4,
            dataset_size_gib: 2.0,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === LANE GEOMETRY STUDY ===
    for lanes in [16, 32, 64] {
        for subgroup in [4, 8, 16] {
            if subgroup <= lanes {
                candidates.push(CandidateProfile {
                    name: format!("lanes-{}-sub{}", lanes, subgroup),
                    lanes,
                    registers: 32,
                    program_length: 112,
                    execution_passes: 4,
                    scratchpad_bytes: 49152,
                    dataset_reads_per_pass: 8,
                    dependent_reads_per_pass: 8,
                    cross_lane_rounds: 2,
                    subgroup_size: subgroup,
                    alu_work_between_reads: 0,
                    scratch_ops_per_pass: 4,
                    dataset_size_gib: 2.0,
                    instruction_weights: InstructionWeights::default(),
                });
            }
        }
    }

    // === DATASET SIZE STUDY ===
    for size in [1.0, 1.5, 2.0, 3.0, 4.0] {
        candidates.push(CandidateProfile {
            name: format!("dataset-{}g", size),
            lanes: 32,
            registers: 32,
            program_length: 112,
            execution_passes: 4,
            scratchpad_bytes: 49152,
            dataset_reads_per_pass: 8,
            dependent_reads_per_pass: 8,
            cross_lane_rounds: 2,
            subgroup_size: 8,
            alu_work_between_reads: 0,
            scratch_ops_per_pass: 4,
            dataset_size_gib: size,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === ALU WORK BETWEEN READS STUDY ===
    // More ALU work between dependent reads reduces HBM advantage
    for alu_between in [0, 4, 8, 16, 24, 32] {
        candidates.push(CandidateProfile {
            name: format!("alu-between-{}", alu_between),
            lanes: 32,
            registers: 32,
            program_length: 112,
            execution_passes: 4,
            scratchpad_bytes: 49152,
            dataset_reads_per_pass: 8,
            dependent_reads_per_pass: 8,
            cross_lane_rounds: 2,
            subgroup_size: 8,
            alu_work_between_reads: alu_between,
            scratch_ops_per_pass: 4,
            dataset_size_gib: 2.0,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === CROSS-LANE MIXING STUDY ===
    for cross_lane in [1, 2, 3, 4] {
        candidates.push(CandidateProfile {
            name: format!("crosslane-{}", cross_lane),
            lanes: 32,
            registers: 32,
            program_length: 112,
            execution_passes: 4,
            scratchpad_bytes: 49152,
            dataset_reads_per_pass: 8,
            dependent_reads_per_pass: 8,
            cross_lane_rounds: cross_lane,
            subgroup_size: 8,
            alu_work_between_reads: 0,
            scratch_ops_per_pass: 4,
            dataset_size_gib: 2.0,
            instruction_weights: InstructionWeights::default(),
        });
    }

    // === INSTRUCTION WEIGHT STUDY ===
    // Higher multiply weight (more compute-bound)
    candidates.push(CandidateProfile {
        name: "weights-heavy-mul".to_string(),
        lanes: 32,
        registers: 32,
        program_length: 112,
        execution_passes: 4,
        scratchpad_bytes: 49152,
        dataset_reads_per_pass: 8,
        dependent_reads_per_pass: 8,
        cross_lane_rounds: 2,
        subgroup_size: 8,
        alu_work_between_reads: 0,
        scratch_ops_per_pass: 4,
        dataset_size_gib: 2.0,
        instruction_weights: InstructionWeights {
            arithmetic: 15,
            bitwise: 15,
            rotation: 10,
            multiplication: 30,
            bit_manipulation: 15,
            scratch_memory: 10,
            cross_lane: 5,
        },
    });

    // Higher scratch weight (more shared-memory-bound)
    candidates.push(CandidateProfile {
        name: "weights-heavy-scratch".to_string(),
        lanes: 32,
        registers: 32,
        program_length: 112,
        execution_passes: 4,
        scratchpad_bytes: 49152,
        dataset_reads_per_pass: 8,
        dependent_reads_per_pass: 8,
        cross_lane_rounds: 2,
        subgroup_size: 8,
        alu_work_between_reads: 0,
        scratch_ops_per_pass: 8,
        dataset_size_gib: 2.0,
        instruction_weights: InstructionWeights {
            arithmetic: 15,
            bitwise: 15,
            rotation: 10,
            multiplication: 10,
            bit_manipulation: 10,
            scratch_memory: 35,
            cross_lane: 5,
        },
    });

    // Higher cross-lane weight
    candidates.push(CandidateProfile {
        name: "weights-heavy-lane".to_string(),
        lanes: 32,
        registers: 32,
        program_length: 112,
        execution_passes: 4,
        scratchpad_bytes: 49152,
        dataset_reads_per_pass: 8,
        dependent_reads_per_pass: 8,
        cross_lane_rounds: 4,
        subgroup_size: 8,
        alu_work_between_reads: 0,
        scratch_ops_per_pass: 4,
        dataset_size_gib: 2.0,
        instruction_weights: InstructionWeights {
            arithmetic: 15,
            bitwise: 15,
            rotation: 10,
            multiplication: 10,
            bit_manipulation: 10,
            scratch_memory: 15,
            cross_lane: 25,
        },
    });

    // === COMBINED PROMISING REGIONS ===
    // Based on initial analysis: more registers, more dep reads, moderate scratch
    candidates.push(CandidateProfile {
        name: "combined-reg48-depreads12".to_string(),
        lanes: 32,
        registers: 48,
        program_length: 112,
        execution_passes: 4,
        scratchpad_bytes: 49152,
        dataset_reads_per_pass: 12,
        dependent_reads_per_pass: 12,
        cross_lane_rounds: 2,
        subgroup_size: 8,
        alu_work_between_reads: 8,
        scratch_ops_per_pass: 4,
        dataset_size_gib: 2.0,
        instruction_weights: InstructionWeights::default(),
    });

    // More passes, more ALU between reads
    candidates.push(CandidateProfile {
        name: "combined-6pass-alu16".to_string(),
        lanes: 32,
        registers: 32,
        program_length: 96,
        execution_passes: 6,
        scratchpad_bytes: 49152,
        dataset_reads_per_pass: 8,
        dependent_reads_per_pass: 8,
        cross_lane_rounds: 3,
        subgroup_size: 8,
        alu_work_between_reads: 16,
        scratch_ops_per_pass: 6,
        dataset_size_gib: 2.0,
        instruction_weights: InstructionWeights {
            arithmetic: 20,
            bitwise: 20,
            rotation: 15,
            multiplication: 20,
            bit_manipulation: 10,
            scratch_memory: 10,
            cross_lane: 5,
        },
    });

    // Heavy compute, reduced memory pressure
    candidates.push(CandidateProfile {
        name: "combined-heavy-compute".to_string(),
        lanes: 32,
        registers: 40,
        program_length: 128,
        execution_passes: 5,
        scratchpad_bytes: 65536,
        dataset_reads_per_pass: 6,
        dependent_reads_per_pass: 6,
        cross_lane_rounds: 3,
        subgroup_size: 8,
        alu_work_between_reads: 24,
        scratch_ops_per_pass: 6,
        dataset_size_gib: 2.0,
        instruction_weights: InstructionWeights {
            arithmetic: 20,
            bitwise: 15,
            rotation: 15,
            multiplication: 25,
            bit_manipulation: 10,
            scratch_memory: 10,
            cross_lane: 5,
        },
    });

    // Low-memory, high-compute design
    candidates.push(CandidateProfile {
        name: "combined-lowmem-compute".to_string(),
        lanes: 32,
        registers: 40,
        program_length: 96,
        execution_passes: 6,
        scratchpad_bytes: 32768,
        dataset_reads_per_pass: 4,
        dependent_reads_per_pass: 4,
        cross_lane_rounds: 3,
        subgroup_size: 8,
        alu_work_between_reads: 32,
        scratch_ops_per_pass: 8,
        dataset_size_gib: 1.5,
        instruction_weights: InstructionWeights {
            arithmetic: 20,
            bitwise: 15,
            rotation: 15,
            multiplication: 25,
            bit_manipulation: 10,
            scratch_memory: 10,
            cross_lane: 5,
        },
    });

    // === HBM-RESISTANT DESIGNS ===
    // Maximum dependency chains, minimal external reads
    candidates.push(CandidateProfile {
        name: "hbm-resistant-maxdep".to_string(),
        lanes: 32,
        registers: 48,
        program_length: 128,
        execution_passes: 6,
        scratchpad_bytes: 65536,
        dataset_reads_per_pass: 4,
        dependent_reads_per_pass: 4,
        cross_lane_rounds: 4,
        subgroup_size: 8,
        alu_work_between_reads: 48,
        scratch_ops_per_pass: 8,
        dataset_size_gib: 2.0,
        instruction_weights: InstructionWeights {
            arithmetic: 20,
            bitwise: 15,
            rotation: 15,
            multiplication: 25,
            bit_manipulation: 10,
            scratch_memory: 10,
            cross_lane: 5,
        },
    });

    // Scratch-heavy, minimal dataset
    candidates.push(CandidateProfile {
        name: "hbm-resistant-scratch-heavy".to_string(),
        lanes: 32,
        registers: 40,
        program_length: 112,
        execution_passes: 5,
        scratchpad_bytes: 65536,
        dataset_reads_per_pass: 4,
        dependent_reads_per_pass: 4,
        cross_lane_rounds: 3,
        subgroup_size: 8,
        alu_work_between_reads: 32,
        scratch_ops_per_pass: 12,
        dataset_size_gib: 1.5,
        instruction_weights: InstructionWeights {
            arithmetic: 15,
            bitwise: 15,
            rotation: 10,
            multiplication: 15,
            bit_manipulation: 10,
            scratch_memory: 30,
            cross_lane: 5,
        },
    });

    candidates
}
