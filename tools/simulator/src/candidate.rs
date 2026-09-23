use serde::{Deserialize, Serialize};

/// A candidate parameter set for the hardware-profile search.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateProfile {
    pub name: String,
    pub lanes: usize,
    pub registers: usize,
    pub program_length: usize,
    pub execution_passes: usize,
    pub scratchpad_bytes: usize,
    pub dataset_reads_per_pass: usize,
    pub dependent_reads_per_pass: usize,
    pub cross_lane_rounds: usize,
    pub subgroup_size: usize,
    pub alu_work_between_reads: usize,
    pub scratch_ops_per_pass: usize,
    pub dataset_size_gib: f64,
    pub instruction_weights: InstructionWeights,
}

/// Instruction group weights for opcode distribution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstructionWeights {
    pub arithmetic: u32,
    pub bitwise: u32,
    pub rotation: u32,
    pub multiplication: u32,
    pub bit_manipulation: u32,
    pub scratch_memory: u32,
    pub cross_lane: u32,
}

impl InstructionWeights {
    pub fn total(&self) -> u32 {
        self.arithmetic
            + self.bitwise
            + self.rotation
            + self.multiplication
            + self.bit_manipulation
            + self.scratch_memory
            + self.cross_lane
    }
}

impl Default for InstructionWeights {
    fn default() -> Self {
        Self {
            arithmetic: 25,
            bitwise: 20,
            rotation: 15,
            multiplication: 15,
            bit_manipulation: 10,
            scratch_memory: 12,
            cross_lane: 3,
        }
    }
}

/// Baseline V1 candidate.
pub fn baseline_v1() -> CandidateProfile {
    CandidateProfile {
        name: "BASELINE-V1".to_string(),
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
        instruction_weights: InstructionWeights::default(),
    }
}

/// Computes the per-hash cycle estimate for a candidate on a given architecture.
///
/// This models the dominant cost factors:
/// 1. ALU work (instructions × passes)
/// 2. Scratchpad access latency
/// 3. Dependent dataset read chains (SERIALIZED - bandwidth doesn't help)
/// 4. Cross-lane mixing overhead
///
/// Returns estimated cycles per hash.
pub fn estimate_cycles_per_hash(
    candidate: &CandidateProfile,
    arch: &super::GpuArchitecture,
) -> f64 {
    let lanes = candidate.lanes as f64;
    let regs = candidate.registers as f64;
    let program_len = candidate.program_length as f64;
    let passes = candidate.execution_passes as f64;
    let dep_reads = candidate.dependent_reads_per_pass as f64;
    let cross_lane = candidate.cross_lane_rounds as f64;

    // ALU throughput: INT32 ops per clock per CU
    // NVIDIA: 128 INT32 ops/clock/SM (Ampere/Ada)
    // AMD RDNA: 64 INT32 ops/clock/CU
    let int32_per_clock_per_cu = match arch.vendor {
        super::GpuVendor::Nvidia => 128.0,
        super::GpuVendor::Amd => 64.0,
        super::GpuVendor::Intel => 64.0,
    };

    // Register pressure: how many waves fit per CU
    let regs_per_thread = regs;
    let reg_file_bytes_per_cu = arch.register_file_per_cu_kb * 1024.0;
    let bytes_per_thread = regs_per_thread * 4.0;
    let max_threads_by_regs = (reg_file_bytes_per_cu / bytes_per_thread).floor();

    // Shared memory pressure
    let smem_bytes_per_workgroup = candidate.scratchpad_bytes as f64;
    let smem_per_cu = arch.shared_memory_per_cu_kb * 1024.0;
    let max_workgroups_by_smem = (smem_per_cu / smem_bytes_per_workgroup).floor().max(1.0);

    // Threads per workgroup = lanes
    let threads_per_workgroup = lanes;
    let max_workgroups_by_threads = (arch.max_threads_per_cu as f64 / threads_per_workgroup)
        .floor()
        .max(1.0);

    // Occupancy: limited by whichever is more restrictive
    let active_workgroups_per_cu = max_workgroups_by_smem
        .min(max_workgroups_by_threads)
        .min(max_threads_by_regs / threads_per_workgroup);

    // ALU cycles per hash
    // Total ALU ops = program_len × passes × lanes
    // ALU throughput per CU = int32_per_clock × active_workgroups (pipeline parallelism)
    let alu_ops_per_hash = program_len * passes * lanes;
    let alu_throughput_per_clock = int32_per_clock_per_cu * active_workgroups_per_cu.min(4.0);
    let alu_cycles = alu_ops_per_hash / alu_throughput_per_clock;

    // Scratchpad access: ~4 cycles if in shared memory, ~100+ if spilled
    let scratch_latency = if smem_bytes_per_workgroup <= smem_per_cu {
        4.0
    } else {
        200.0 // Spill to local memory - very expensive
    };
    let scratch_ops = candidate.scratch_ops_per_pass as f64 * passes;
    let scratch_cycles = scratch_ops * scratch_latency;

    // DEPENDENT DATASET READS - THE KEY MECHANISM
    // Each read depends on the previous read's result
    // This SERIALIZES memory access - bandwidth doesn't help
    //
    // Critical insight: HBM's advantage is BANDWIDTH, not LATENCY
    // Both HBM and GDDR have similar latency (~400-600 cycles)
    // Dependent chains mean only one read in flight at a time
    // So HBM bandwidth advantage is largely negated
    let dram_latency_cycles = 400.0; // Similar for HBM and GDDR
    let _l2_latency_cycles = 100.0;

    // For dependent reads, effective latency is dominated by DRAM latency
    // because each read must complete before next address is known
    let dep_chain_cycles_per_pass = dep_reads * dram_latency_cycles;

    // ALU work between dependent reads extends the chain
    let alu_between_reads_cycles =
        candidate.alu_work_between_reads as f64 * dep_reads / int32_per_clock_per_cu;

    // Total dependent chain = (read latency + ALU work) × reads × passes
    let total_dep_chain = (dep_chain_cycles_per_pass + alu_between_reads_cycles) * passes;

    // Cross-lane mixing: requires synchronization
    // On GPU: SHFL/DPP is ~4-8 cycles per register
    // On CPU: must use memory ~100 cycles
    let cross_lane_latency_per_reg = if arch.has_native_shuffle { 4.0 } else { 100.0 };
    let cross_lane_regs = regs * cross_lane * passes;
    let cross_lane_cycles = cross_lane_regs * cross_lane_latency_per_reg / lanes;

    // Total cycles: sum of all components
    // The dependent read chain is the serial bottleneck
    let compute_bound = alu_cycles + scratch_cycles + cross_lane_cycles;
    let memory_bound = total_dep_chain;

    // Effective cycles = max(compute, memory) with some overlap
    // Dependent reads can partially overlap with compute (but not fully)
    let overlap_factor = 0.7; // 70% of compute can overlap with memory
    let effective_cycles = memory_bound + compute_bound * (1.0 - overlap_factor);

    // Adjust for occupancy: lower occupancy = more latency hiding opportunity
    let occupancy_factor = if active_workgroups_per_cu >= 2.0 {
        1.0
    } else if active_workgroups_per_cu >= 1.0 {
        1.2 // Slightly worse with low occupancy
    } else {
        2.0 // Very bad occupancy
    };

    effective_cycles * occupancy_factor
}

/// Computes estimated hash rate in hashes per second.
pub fn estimate_hash_rate(candidate: &CandidateProfile, arch: &super::GpuArchitecture) -> f64 {
    let cycles_per_hash = estimate_cycles_per_hash(candidate, arch);
    let clock_hz = arch.boost_clock_mhz * 1_000_000.0;
    let cus = arch.compute_units as f64;

    // Hashes per second = (CUs × clock) / cycles_per_hash
    // But we need to account for parallelism within each CU
    let hashes_per_second = (cus * clock_hz) / cycles_per_hash;

    // Apply a realistic efficiency factor (0.3-0.7 depending on architecture)
    let efficiency = match arch.arch_class {
        super::ArchClass::GamingGpu => 0.5,
        super::ArchClass::ComputeCard => 0.4,
        super::ArchClass::DatacenterGpu => 0.45,
        super::ArchClass::Cpu => 0.15,
    };

    hashes_per_second * efficiency
}

/// Computes estimated power consumption during mining.
pub fn estimate_power(candidate: &CandidateProfile, arch: &super::GpuArchitecture) -> f64 {
    // Power depends on utilization
    let compute_intensity = candidate.program_length as f64 * candidate.execution_passes as f64;
    let memory_intensity =
        candidate.dependent_reads_per_pass as f64 * candidate.execution_passes as f64;

    // Higher compute intensity = more power
    // Higher memory intensity = moderate power (memory subsystem)
    let compute_fraction = compute_intensity / (compute_intensity + memory_intensity * 10.0);
    let memory_fraction = 1.0 - compute_fraction;

    // Typical utilization: 60-85% of TDP
    let base_utilization = 0.7;
    let compute_power = arch.tdp_watts * base_utilization * compute_fraction;
    let memory_power = arch.tdp_watts * 0.3 * memory_fraction;

    compute_power + memory_power
}
