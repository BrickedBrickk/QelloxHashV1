/// 256-bit hash type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Hash256(pub [u8; 32]);

impl Hash256 {
    pub const ZERO: Self = Self([0u8; 32]);

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// A single VM instruction encoded as a 32-bit word.
///
/// Layout (little-endian bit positions):
///   [7:0]   opcode
///   [12:8]  dst register (0-31)
///   [17:13] src_a register (0-31)
///   [22:18] src_b register (0-31)
///   [27:23] imm5 (5-bit immediate / sub-opcode)
///   [31:28] flags (lane interaction, memory, etc.)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Instruction(pub u32);

impl Instruction {
    #[must_use]
    pub fn opcode(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    #[must_use]
    pub fn dst(self) -> u8 {
        ((self.0 >> 8) & 0x1F) as u8
    }

    #[must_use]
    pub fn src_a(self) -> u8 {
        ((self.0 >> 13) & 0x1F) as u8
    }

    #[must_use]
    pub fn src_b(self) -> u8 {
        ((self.0 >> 18) & 0x1F) as u8
    }

    #[must_use]
    pub fn imm5(self) -> u8 {
        ((self.0 >> 23) & 0x1F) as u8
    }

    #[must_use]
    pub fn flags(self) -> u8 {
        ((self.0 >> 28) & 0x0F) as u8
    }

    #[must_use]
    pub fn new(opcode: u8, dst: u8, src_a: u8, src_b: u8, imm5: u8, flags: u8) -> Self {
        let word = (opcode as u32)
            | ((dst as u32 & 0x1F) << 8)
            | ((src_a as u32 & 0x1F) << 13)
            | ((src_b as u32 & 0x1F) << 18)
            | ((imm5 as u32 & 0x1F) << 23)
            | ((flags as u32 & 0x0F) << 28);
        Self(word)
    }
}

/// Generated program: a sequence of instructions.
#[derive(Clone, Debug)]
pub struct Program {
    pub instructions: Vec<Instruction>,
    pub cross_lane_map: Vec<u8>,
}

/// VM execution state for one candidate hash.
#[derive(Clone, Debug)]
pub struct VmState {
    /// registers[lane][register_index]
    pub registers: [[u32; 32]; 32],
    /// Scratchpad memory (shared across lanes, u32-indexed).
    pub scratchpad: Vec<u32>,
}

/// Candidate parameter set for algorithm search.
#[derive(Clone, Debug)]
pub struct CandidateProfile {
    pub name: &'static str,
    pub lanes: usize,
    pub registers: usize,
    pub program_length: usize,
    pub execution_passes: usize,
    pub scratchpad_bytes: usize,
    pub dataset_reads_per_pass: usize,
    pub cross_lane_rounds: usize,
    pub subgroup_size: usize,
    pub dependency_depth: usize,
}

/// Default candidate profile.
pub const DEFAULT_CANDIDATE: CandidateProfile = CandidateProfile {
    name: "QelloxHashV1-default",
    lanes: 32,
    registers: 32,
    program_length: 112,
    execution_passes: 4,
    scratchpad_bytes: 49_152,
    dataset_reads_per_pass: 8,
    cross_lane_rounds: 2,
    subgroup_size: 8,
    dependency_depth: 4,
};

/// Alternative candidates for comparison.
pub const CANDIDATES: &[CandidateProfile] = &[
    DEFAULT_CANDIDATE,
    CandidateProfile {
        name: "QelloxHashV1-lite",
        lanes: 16,
        registers: 32,
        program_length: 64,
        execution_passes: 3,
        scratchpad_bytes: 32_768,
        dataset_reads_per_pass: 4,
        cross_lane_rounds: 1,
        subgroup_size: 8,
        dependency_depth: 3,
    },
    CandidateProfile {
        name: "QelloxHashV1-heavy",
        lanes: 32,
        registers: 32,
        program_length: 128,
        execution_passes: 6,
        scratchpad_bytes: 65_536,
        dataset_reads_per_pass: 12,
        cross_lane_rounds: 3,
        subgroup_size: 8,
        dependency_depth: 6,
    },
    CandidateProfile {
        name: "QelloxHashV1-wide",
        lanes: 64,
        registers: 32,
        program_length: 96,
        execution_passes: 3,
        scratchpad_bytes: 49_152,
        dataset_reads_per_pass: 6,
        cross_lane_rounds: 2,
        subgroup_size: 16,
        dependency_depth: 4,
    },
];
