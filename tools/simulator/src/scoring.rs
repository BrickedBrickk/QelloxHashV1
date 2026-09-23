use serde::{Deserialize, Serialize};

use crate::architecture::{ArchClass, GpuArchitecture, GpuVendor};
use crate::candidate::{estimate_hash_rate, estimate_power, CandidateProfile};

/// Detailed scoring result for a candidate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateScore {
    pub candidate_name: String,
    pub total_score: f64,
    pub gaming_gpu_score: f64,
    pub amd_nvidia_balance: f64,
    pub compute_card_penalty: f64,
    pub cpu_penalty: f64,
    pub verification_score: f64,
    pub vram_score: f64,
    pub riser_score: f64,
    pub generation_scaling_score: f64,
    pub hbm_penalty: f64,
    pub architecture_results: Vec<ArchResult>,
    pub rejected: bool,
    pub rejection_reason: Option<String>,
}

/// Per-architecture result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchResult {
    pub name: String,
    pub arch_class: String,
    pub vendor: String,
    pub hash_rate_hs: f64,
    pub power_watts: f64,
    pub efficiency_kh_w: f64,
    pub normalized_throughput: f64,
}

/// Scoring weights for the candidate evaluation.
#[derive(Clone, Debug)]
pub struct ScoringWeights {
    /// Weight for gaming GPU throughput
    pub gaming_throughput: f64,
    /// Weight for AMD/NVIDIA balance
    pub vendor_balance: f64,
    /// Penalty for compute card competitiveness
    pub compute_card_penalty: f64,
    /// Penalty for CPU competitiveness
    pub cpu_penalty: f64,
    /// Weight for reasonable verification cost
    pub verification: f64,
    /// Weight for moderate VRAM
    pub vram: f64,
    /// Weight for riser compatibility
    pub riser: f64,
    /// Weight for moderate generation scaling
    pub generation_scaling: f64,
    /// Penalty for HBM-dominated designs
    pub hbm_penalty: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            gaming_throughput: 0.20,
            vendor_balance: 0.15,
            compute_card_penalty: 0.25,
            cpu_penalty: 0.10,
            verification: 0.05,
            vram: 0.05,
            riser: 0.05,
            generation_scaling: 0.05,
            hbm_penalty: 0.10,
        }
    }
}

/// Evaluates a candidate against the full architecture database.
pub fn score_candidate(
    candidate: &CandidateProfile,
    architectures: &[GpuArchitecture],
    weights: &ScoringWeights,
) -> CandidateScore {
    let mut arch_results = Vec::new();

    // Compute hash rates for all architectures
    let mut gaming_hash_rates = Vec::new();
    let mut compute_hash_rates = Vec::new();
    let mut nvidia_gaming_rates = Vec::new();
    let mut amd_gaming_rates = Vec::new();

    for arch in architectures {
        let hash_rate = estimate_hash_rate(candidate, arch);
        let power = estimate_power(candidate, arch);
        let efficiency = if power > 0.0 {
            hash_rate / power * 1000.0 // kH/W
        } else {
            0.0
        };

        arch_results.push(ArchResult {
            name: arch.name.clone(),
            arch_class: format!("{:?}", arch.arch_class),
            vendor: format!("{:?}", arch.vendor),
            hash_rate_hs: hash_rate,
            power_watts: power,
            efficiency_kh_w: efficiency,
            normalized_throughput: 0.0, // Filled later
        });

        match arch.arch_class {
            ArchClass::GamingGpu => {
                gaming_hash_rates.push((arch.clone(), hash_rate));
                match arch.vendor {
                    GpuVendor::Nvidia => nvidia_gaming_rates.push(hash_rate),
                    GpuVendor::Amd => amd_gaming_rates.push(hash_rate),
                    _ => {}
                }
            }
            ArchClass::ComputeCard => {
                compute_hash_rates.push((arch.clone(), hash_rate));
            }
            ArchClass::Cpu => {}
            _ => {}
        }
    }

    // Normalize throughputs relative to RTX 3060
    let rtx3060_rate = arch_results
        .iter()
        .find(|r| r.name == "RTX 3060")
        .map(|r| r.hash_rate_hs)
        .unwrap_or(1.0);

    for result in &mut arch_results {
        result.normalized_throughput = result.hash_rate_hs / rtx3060_rate;
    }

    // === SCORING COMPONENTS ===

    // 1. Gaming GPU throughput score
    // Reward candidates where gaming GPUs have good throughput
    let avg_gaming_rate: f64 = if gaming_hash_rates.is_empty() {
        0.0
    } else {
        gaming_hash_rates.iter().map(|(_, r)| r).sum::<f64>() / gaming_hash_rates.len() as f64
    };
    let gaming_gpu_score = (avg_gaming_rate / 1_000_000.0).min(1.0); // Normalize to ~1.0 at 1 MH/s

    // 2. AMD/NVIDIA balance
    // Reward candidates where both vendors perform similarly
    let vendor_balance = if !nvidia_gaming_rates.is_empty() && !amd_gaming_rates.is_empty() {
        let avg_nvidia: f64 =
            nvidia_gaming_rates.iter().sum::<f64>() / nvidia_gaming_rates.len() as f64;
        let avg_amd: f64 = amd_gaming_rates.iter().sum::<f64>() / amd_gaming_rates.len() as f64;
        if avg_nvidia > avg_amd {
            avg_amd / avg_nvidia
        } else {
            avg_nvidia / avg_amd
        } // 1.0 = perfect balance
    } else {
        0.5
    };

    // 3. Compute card penalty
    // Penalize if compute cards have competitive hash/$ with gaming
    // We want compute cards to have POOR hash/$ despite competitive raw hash rate
    let compute_card_penalty = if !compute_hash_rates.is_empty() && !gaming_hash_rates.is_empty() {
        // Compare hash/$ of compute cards vs midrange gaming
        let best_compute_hash_per_dollar = compute_hash_rates
            .iter()
            .map(|(a, r)| r / a.approximate_price_usd)
            .fold(0.0_f64, f64::max);
        let midrange_gaming_hash_per_dollar: Vec<f64> = gaming_hash_rates
            .iter()
            .filter(|(a, _)| a.approximate_price_usd < 600.0)
            .map(|(a, r)| r / a.approximate_price_usd)
            .collect();
        let avg_midrange_hpd = if midrange_gaming_hash_per_dollar.is_empty() {
            1.0
        } else {
            midrange_gaming_hash_per_dollar.iter().sum::<f64>()
                / midrange_gaming_hash_per_dollar.len() as f64
        };

        // We want compute cards to have WORSE hash/$ than gaming
        // Score: 1.0 if compute hash/$ is 0.3x midrange, 0.0 if compute equals midrange
        let ratio = best_compute_hash_per_dollar / avg_midrange_hpd.max(1.0);
        if ratio < 0.3 {
            1.0 // Excellent: compute cards have terrible hash/$
        } else if ratio < 0.5 {
            0.8 // Good: compute cards have poor hash/$
        } else if ratio < 0.7 {
            0.5 // Acceptable
        } else if ratio < 1.0 {
            0.2 // Poor: compute cards are competitive on hash/$
        } else {
            0.0 // Bad: compute cards beat gaming on hash/$
        }
    } else {
        0.5
    };

    // 4. CPU penalty
    // CPUs should be dramatically worse
    let cpu_penalty = {
        let cpu_rate = arch_results
            .iter()
            .find(|r| r.arch_class == "Cpu")
            .map(|r| r.hash_rate_hs)
            .unwrap_or(0.0);
        let gaming_avg = avg_gaming_rate.max(1.0);
        let ratio = cpu_rate / gaming_avg;
        if ratio < 0.01 {
            1.0 // Excellent: CPU is 100x+ worse
        } else if ratio < 0.05 {
            0.8 // Good
        } else if ratio < 0.1 {
            0.5 // Acceptable
        } else {
            0.0 // Bad
        }
    };

    // 5. Verification score
    // Light verification should be reasonable (< 1ms target)
    let verification_cost_us = estimate_verification_cost_us(candidate);
    let verification_score = if verification_cost_us < 200.0 {
        1.0
    } else if verification_cost_us < 500.0 {
        0.8
    } else if verification_cost_us < 1000.0 {
        0.5
    } else {
        0.0
    };

    // 6. VRAM score
    // 2-4 GiB is ideal for consumer GPUs
    let vram_gib = candidate.dataset_size_gib;
    let vram_score = if (1.5..=4.0).contains(&vram_gib) {
        1.0
    } else if (1.0..=6.0).contains(&vram_gib) {
        0.7
    } else {
        0.3
    };

    // 7. Riser score
    // <2% loss on x1 vs x16 after initialization
    let per_job_bytes = estimate_per_job_bytes(candidate);
    let riser_score = if per_job_bytes < 1024 {
        1.0
    } else if per_job_bytes < 2048 {
        0.8
    } else {
        0.5
    };

    // 8. Generation scaling score
    // Moderate improvement between generations (not exponential)
    let gen_scaling_score = {
        let rtx3060 = arch_results
            .iter()
            .find(|r| r.name == "RTX 3060")
            .map(|r| r.normalized_throughput)
            .unwrap_or(1.0);
        let _rtx4070 = arch_results
            .iter()
            .find(|r| r.name == "RTX 4070")
            .map(|r| r.normalized_throughput)
            .unwrap_or(2.0);
        let rtx5070 = arch_results
            .iter()
            .find(|r| r.name == "RTX 5070")
            .map(|r| r.normalized_throughput)
            .unwrap_or(2.5);
        // We want moderate scaling: 1.5-2.5x from 3060 to 5070
        let total_scaling = rtx5070 / rtx3060;
        if (1.5..=2.5).contains(&total_scaling) {
            1.0
        } else if (1.2..=3.0).contains(&total_scaling) {
            0.7
        } else {
            0.3
        }
    };

    // 9. HBM penalty
    // Penalize if HBM cards dominate due to bandwidth
    let hbm_penalty = {
        let hbm_cards: Vec<f64> = compute_hash_rates
            .iter()
            .filter(|(a, _)| {
                matches!(
                    a.memory_type,
                    crate::architecture::MemoryType::Hbm2
                        | crate::architecture::MemoryType::Hbm2e
                        | crate::architecture::MemoryType::Hbm3
                )
            })
            .map(|(_, r)| *r)
            .collect();
        let gddr_cards: Vec<f64> = gaming_hash_rates
            .iter()
            .filter(|(a, _)| {
                matches!(
                    a.memory_type,
                    crate::architecture::MemoryType::Gddr6
                        | crate::architecture::MemoryType::Gddr6x
                )
            })
            .map(|(_, r)| *r)
            .collect();

        if !hbm_cards.is_empty() && !gddr_cards.is_empty() {
            let avg_hbm = hbm_cards.iter().sum::<f64>() / hbm_cards.len() as f64;
            let avg_gddr = gddr_cards.iter().sum::<f64>() / gddr_cards.len() as f64;
            let ratio = avg_hbm / avg_gddr.max(1.0);
            if ratio < 1.0 {
                1.0 // HBM is worse than GDDR
            } else if ratio < 1.5 {
                0.7 // Acceptable
            } else {
                0.0 // HBM dominates
            }
        } else {
            0.5
        }
    };

    // === TOTAL SCORE ===
    let total_score = weights.gaming_throughput * gaming_gpu_score
        + weights.vendor_balance * vendor_balance
        + weights.compute_card_penalty * compute_card_penalty
        + weights.cpu_penalty * cpu_penalty
        + weights.verification * verification_score
        + weights.vram * vram_score
        + weights.riser * riser_score
        + weights.generation_scaling * gen_scaling_score
        + weights.hbm_penalty * hbm_penalty;

    // Check for automatic rejection
    let (rejected, rejection_reason) = if compute_card_penalty < 0.1 {
        (true, Some("Compute cards dominate gaming GPUs".to_string()))
    } else if cpu_penalty < 0.1 {
        (
            true,
            Some("CPU is too competitive with gaming GPUs".to_string()),
        )
    } else if vram_score < 0.3 {
        (true, Some("VRAM requirement unreasonable".to_string()))
    } else {
        (false, None)
    };

    CandidateScore {
        candidate_name: candidate.name.clone(),
        total_score,
        gaming_gpu_score,
        amd_nvidia_balance: vendor_balance,
        compute_card_penalty,
        cpu_penalty,
        verification_score,
        vram_score,
        riser_score,
        generation_scaling_score: gen_scaling_score,
        hbm_penalty,
        architecture_results: arch_results,
        rejected,
        rejection_reason,
    }
}

/// Estimates verification cost in microseconds.
fn estimate_verification_cost_us(candidate: &CandidateProfile) -> f64 {
    // Verification requires:
    // 1. SHA-256 seed: ~1 us
    // 2. Program generation: ~112 SHA-256 ≈ 22 us
    // 3. Register init: ~1024 SHA-256 ≈ 200 us
    // 4. Scratchpad init: ~1536 SHA-256 ≈ 300 us
    // 5. VM execution: program_length × passes × lanes / cpu_throughput
    let sha256_per_us = 5000.0; // ~5k SHA-256/s on modern CPU
    let seed_cost = 1.0;
    let program_cost = candidate.program_length as f64 / sha256_per_us;
    let reg_init_cost = (candidate.lanes * candidate.registers) as f64 / sha256_per_us;
    let scratch_init_cost = (candidate.scratchpad_bytes / 4) as f64 / 8.0 / sha256_per_us;
    let vm_cost = (candidate.program_length * candidate.execution_passes) as f64 / 10_000.0; // ~10k ops/us on CPU

    (seed_cost + program_cost + reg_init_cost + scratch_init_cost + vm_cost) * 1_000.0
    // Convert to us
}

/// Estimates per-job PCIe bytes.
fn estimate_per_job_bytes(candidate: &CandidateProfile) -> usize {
    // Header + nonce: 88 bytes
    // Program: program_length × 4 bytes
    // Cross-lane map: lanes bytes
    // Result: 32 bytes
    88 + candidate.program_length * 4 + candidate.lanes + 32
}
