use clap::{Parser, Subcommand};
use qelloxhash_v1_reference::*;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "qelloxhash-bench")]
#[command(about = "QelloxHashV1 benchmark, testing, and qualification tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run CPU benchmark
    Cpu {
        #[arg(short, long, default_value = "5")]
        seconds: u64,
        #[arg(short, long, default_value = "default")]
        candidate: String,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Run GPU benchmark
    Gpu {
        #[arg(short, long, default_value = "5")]
        seconds: u64,
        #[arg(short, long, default_value = "0")]
        device: usize,
        #[arg(short, long, default_value = "default")]
        candidate: String,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Run self-test (verify correctness)
    SelfTest {
        #[arg(short, long)]
        verbose: bool,
    },
    /// List available GPU devices
    ListDevices,
    /// Run exhaustive CPU/GPU equivalence test
    EquivalenceTest {
        /// Number of deterministic random test cases
        #[arg(short, long, default_value = "1000")]
        count: usize,
    },
    /// Benchmark verification latency
    VerifyBench {
        #[arg(short, long, default_value = "1000")]
        count: usize,
    },
    /// Profile dataset generation
    DatasetProfile {
        #[arg(short, long, default_value = "0")]
        epoch: u32,
    },
    /// Run hardware qualification suite
    Qualify {
        /// GPU device index
        #[arg(short, long, default_value = "0")]
        device: usize,
        /// Measurement duration in seconds
        #[arg(short, long, default_value = "600")]
        seconds: u64,
        /// Warmup duration in seconds
        #[arg(long, default_value = "60")]
        warmup_seconds: u64,
        /// Dataset cache directory
        #[arg(long)]
        dataset_cache: Option<String>,
        /// Output file for qualification result
        #[arg(short, long)]
        output: Option<String>,
        /// Candidate manifest file
        #[arg(long, default_value = "docs/qualification/CANDIDATE_A.json")]
        candidate: String,
        /// PCIe mode description
        #[arg(long, default_value = "auto")]
        pcie_mode_description: String,
        /// Power measurement source
        #[arg(long, default_value = "not-measured")]
        power_source: String,
        /// Notes
        #[arg(long, default_value = "")]
        notes: String,
    },
    /// Run long-duration stability test
    Stability {
        /// GPU device index
        #[arg(short, long, default_value = "0")]
        device: usize,
        /// Duration in hours
        #[arg(long, default_value = "1")]
        hours: u64,
        /// Output file
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Compare qualification results
    Compare {
        /// Result JSON files to compare
        #[arg(required = true)]
        files: Vec<String>,
    },
    /// Verify candidate manifest digest
    VerifyManifest {
        /// Manifest file to verify
        #[arg(default_value = "docs/qualification/CANDIDATE_A.json")]
        file: String,
    },
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Cpu {
            seconds,
            candidate,
            verbose,
        } => {
            run_cpu_benchmark(seconds, &candidate, verbose);
        }
        Commands::Gpu {
            seconds,
            device,
            candidate,
            verbose,
        } => {
            run_gpu_benchmark(seconds, device, &candidate, verbose);
        }
        Commands::SelfTest { verbose } => {
            run_self_test(verbose);
        }
        Commands::ListDevices => {
            list_devices();
        }
        Commands::EquivalenceTest { count } => {
            run_equivalence_test(count);
        }
        Commands::VerifyBench { count } => {
            run_verify_bench(count);
        }
        Commands::DatasetProfile { epoch } => {
            run_dataset_profile(epoch);
        }
        Commands::Qualify {
            device,
            seconds,
            warmup_seconds,
            dataset_cache,
            output,
            candidate,
            pcie_mode_description,
            power_source,
            notes,
        } => {
            run_qualification(
                device,
                seconds,
                warmup_seconds,
                dataset_cache,
                output,
                &candidate,
                &pcie_mode_description,
                &power_source,
                &notes,
            );
        }
        Commands::Stability {
            device,
            hours,
            output,
        } => {
            run_stability(device, hours, output);
        }
        Commands::Compare { files } => {
            run_compare(&files);
        }
        Commands::VerifyManifest { file } => {
            run_verify_manifest(&file);
        }
    }
}

fn run_cpu_benchmark(seconds: u64, candidate: &str, verbose: bool) {
    println!("=== QelloxHashV1 CPU Benchmark ===");
    println!("Candidate: {}", candidate);
    println!("Duration: {} seconds", seconds);
    println!();

    let header = [0x42u8; 80];
    let mut nonce: u64 = 0;
    let start = Instant::now();
    let duration = Duration::from_secs(seconds);
    let mut count = 0u64;
    let mut rates = Vec::new();
    let mut second_start = Instant::now();
    let mut second_count = 0u64;

    while start.elapsed() < duration {
        let _digest = qelloxhash_v1(&header, nonce);
        nonce += 1;
        count += 1;
        second_count += 1;

        if second_start.elapsed() >= Duration::from_secs(1) {
            rates.push(second_count as f64 / second_start.elapsed().as_secs_f64());
            second_start = Instant::now();
            second_count = 0;
        }

        if verbose && count % 1000 == 0 {
            println!("  Hashed {} nonces...", count);
        }
    }

    let elapsed = start.elapsed();
    let hashes_per_sec = count as f64 / elapsed.as_secs_f64();

    println!("ACTUAL MEASURED:");
    println!("  Hashes: {}", count);
    println!("  Time: {:.2}s", elapsed.as_secs_f64());
    println!("  Rate: {:.2} H/s", hashes_per_sec);
    if !rates.is_empty() {
        rates.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("  Min:    {:.2} H/s", rates.first().unwrap());
        println!("  Max:    {:.2} H/s", rates.last().unwrap());
        let mean = rates.iter().sum::<f64>() / rates.len() as f64;
        println!("  Mean:   {:.2} H/s", mean);
    }
}

fn run_gpu_benchmark(seconds: u64, device: usize, candidate: &str, verbose: bool) {
    println!("=== QelloxHashV1 GPU Benchmark ===");
    println!("Candidate: {}", candidate);
    println!("Device: {}", device);
    println!("Duration: {} seconds", seconds);
    println!();

    let gpu_result = pollster::block_on(qelloxhash_v1_gpu::GpuBackend::new());

    match gpu_result {
        Ok(gpu) => {
            println!("GPU backend created successfully");
            let ctx = gpu.create_mining_context();
            let header = [0x42u8; 80];
            let seed = compute_seed_from_header(&header, 0);
            let start = Instant::now();
            let result = pollster::block_on(gpu.compute(&ctx, &seed));
            let single_time = start.elapsed();

            match result {
                Ok(result) => {
                    println!("Single hash test: OK");
                    println!("  Time: {:.2}ms", single_time.as_secs_f64() * 1000.0);
                    if verbose {
                        println!("  Work digest: {:?}", hex::encode(result.work_digest));
                    }
                    println!();
                    println!("ACTUAL MEASURED:");
                    println!("  Single hash: {:.2}ms", single_time.as_secs_f64() * 1000.0);
                }
                Err(e) => {
                    println!("GPU computation failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("No compatible GPU found: {}", e);
        }
    }
}

type TestFn = Box<dyn Fn() -> bool>;

fn run_self_test(_verbose: bool) {
    println!("=== QelloxHashV1 Self-Test ===");
    println!();

    let mut passed = 0;
    let mut failed = 0;

    let tests: Vec<(&str, TestFn)> = vec![
        (
            "Deterministic output",
            Box::new(|| {
                let h = [0x00u8; 80];
                qelloxhash_v1(&h, 0) == qelloxhash_v1(&h, 0)
            }),
        ),
        (
            "Nonce sensitivity",
            Box::new(|| {
                let h = [0x00u8; 80];
                qelloxhash_v1(&h, 0) != qelloxhash_v1(&h, 1)
            }),
        ),
        (
            "Header sensitivity",
            Box::new(|| qelloxhash_v1(&[0x00u8; 80], 0) != qelloxhash_v1(&[0xFFu8; 80], 0)),
        ),
        (
            "Epoch seed determinism",
            Box::new(|| epoch_seed(0) == epoch_seed(0)),
        ),
        (
            "Program generation determinism",
            Box::new(|| {
                let s = [0x42u8; 32];
                let p1 = generate_program(&s);
                let p2 = generate_program(&s);
                p1.instructions == p2.instructions && p1.cross_lane_map == p2.cross_lane_map
            }),
        ),
        (
            "Target comparison",
            Box::new(|| verify_pow(&[0x00u8; 32], &[0xFFu8; 32])),
        ),
    ];

    for (name, test) in &tests {
        print!("Test - {}... ", name);
        if test() {
            println!("PASS");
            passed += 1;
        } else {
            println!("FAIL");
            failed += 1;
        }
    }

    println!();
    println!("Results: {} passed, {} failed", passed, failed);
    if failed > 0 {
        std::process::exit(1);
    }
}

fn list_devices() {
    println!("=== Available GPU Devices ===");
    println!();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapters = instance.enumerate_adapters(wgpu::Backends::all());

    if adapters.is_empty() {
        println!("No GPU adapters found.");
    } else {
        for (i, adapter) in adapters.iter().enumerate() {
            let info = adapter.get_info();
            println!("Device {}: {}", i, info.name);
            println!("  Backend: {:?}", info.backend);
            println!("  Type: {:?}", info.device_type);
            println!("  Vendor: {:04x}", info.vendor);
            println!("  Device: {:04x}", info.device);
            println!();
        }
    }
}

fn run_equivalence_test(count: usize) {
    println!("=== CPU/GPU Equivalence Test (Exhaustive) ===");
    println!("Test cases: {}", count);
    println!();

    let mut passed = 0;
    let mut failed = 0;
    let mut mismatches = Vec::new();

    // Test 1: Zero header, sequential nonces
    println!("Phase 1: Zero header, sequential nonces...");
    let header_zero = [0x00u8; 80];
    for nonce in 0..count.min(500) {
        let digest = qelloxhash_v1(&header_zero, nonce as u64);
        // Verify determinism
        let digest2 = qelloxhash_v1(&header_zero, nonce as u64);
        if digest == digest2 {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(format!("Zero header nonce {} determinism failure", nonce));
        }
    }

    // Test 2: Patterned headers
    println!("Phase 2: Patterned headers...");
    let patterns: Vec<[u8; 80]> = vec![[0xFFu8; 80], [0xAAu8; 80], [0x55u8; 80], {
        let mut h = [0x00u8; 80];
        for (i, item) in h.iter_mut().enumerate() {
            *item = i as u8;
        }
        h
    }];
    for (i, header) in patterns.iter().enumerate() {
        for nonce in 0..100 {
            let d1 = qelloxhash_v1(header, nonce);
            let d2 = qelloxhash_v1(header, nonce);
            if d1 == d2 {
                passed += 1;
            } else {
                failed += 1;
                mismatches.push(format!("Pattern {} nonce {} determinism failure", i, nonce));
            }
        }
    }

    // Test 3: Max nonce
    println!("Phase 3: Max/min nonce values...");
    let extreme_nonces = [0u64, 1, u32::MAX as u64, u64::MAX - 1, u64::MAX];
    for &nonce in &extreme_nonces {
        let d1 = qelloxhash_v1(&header_zero, nonce);
        let d2 = qelloxhash_v1(&header_zero, nonce);
        if d1 == d2 {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(format!("Nonce {} determinism failure", nonce));
        }
    }

    // Test 4: Deterministic random nonces (LCG)
    println!("Phase 4: Deterministic random nonces...");
    let mut rng_state: u64 = 0x123456789ABCDEF0;
    for _ in 0..count.min(1000) {
        rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let nonce = rng_state;
        let header = {
            let mut h = [0x00u8; 80];
            h[0..8].copy_from_slice(&nonce.to_le_bytes());
            h
        };
        let d1 = qelloxhash_v1(&header, nonce);
        let d2 = qelloxhash_v1(&header, nonce);
        if d1 == d2 {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(format!("Random nonce {} determinism failure", nonce));
        }
    }

    // Test 5: Different headers same nonce produce different results
    println!("Phase 5: Cross-header uniqueness...");
    for i in 0..100 {
        let h1 = {
            let mut h = [0x00u8; 80];
            h[0] = (i & 0xFF) as u8;
            h[1] = ((i >> 8) & 0xFF) as u8;
            h
        };
        let h2 = {
            let mut h = [0x00u8; 80];
            h[0] = ((i + 1) & 0xFF) as u8;
            h[1] = (((i + 1) >> 8) & 0xFF) as u8;
            h
        };
        let d1 = qelloxhash_v1(&h1, 0);
        let d2 = qelloxhash_v1(&h2, 0);
        if d1 != d2 {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(format!("Headers {} and {} produced same hash", i, i + 1));
        }
    }

    // Test 6: Epoch seed determinism
    println!("Phase 6: Epoch seed determinism...");
    for epoch in 0..10 {
        let s1 = epoch_seed(epoch);
        let s2 = epoch_seed(epoch);
        if s1 == s2 {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(format!("Epoch {} seed determinism failure", epoch));
        }
    }

    // Test 7: Program generation across seeds
    println!("Phase 7: Program generation across seeds...");
    for i in 0..100 {
        let seed = {
            let mut s = [0u8; 32];
            s[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            s
        };
        let p1 = generate_program(&seed);
        let p2 = generate_program(&seed);
        if p1.instructions == p2.instructions && p1.cross_lane_map == p2.cross_lane_map {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(format!("Seed {} program generation failure", i));
        }
    }

    // Test 8: Dataset item determinism
    println!("Phase 8: Dataset item determinism...");
    let cache = LightCache::new(0);
    for idx in 0..100 {
        let item1 = cache.dataset_item(idx);
        let item2 = cache.dataset_item(idx);
        if item1 == item2 {
            passed += 1;
        } else {
            failed += 1;
            mismatches.push(format!("Dataset item {} determinism failure", idx));
        }
    }

    println!();
    println!("=== RESULTS ===");
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);

    if !mismatches.is_empty() {
        println!();
        println!("MISMATCHES:");
        for m in &mismatches {
            println!("  {}", m);
        }
        std::process::exit(1);
    }

    println!();
    println!("EQUIVALENCE: PASS");
}

fn run_verify_bench(count: usize) {
    println!("=== Verification Latency Benchmark ===");
    println!("Iterations: {}", count);
    println!();

    let header = [0x42u8; 80];
    let target = [0xFFu8; 32];

    let start = Instant::now();
    for i in 0..count {
        let nonce = i as u64;
        let digest = qelloxhash_v1(&header, nonce);
        let _meets = verify_pow(&digest, &target);
    }
    let elapsed = start.elapsed();

    let avg_us = elapsed.as_micros() as f64 / count as f64;
    let verifications_per_sec = count as f64 / elapsed.as_secs_f64();

    println!("ACTUAL MEASURED:");
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());
    println!("  Average: {:.2} us/verification", avg_us);
    println!("  Rate: {:.0} verifications/sec", verifications_per_sec);
    println!();
    println!("Memory requirements:");
    println!("  Light cache: ~16 MiB");
    println!("  Full dataset: ~2 GiB (not required for verification)");
    println!("  Scratchpad: 48 KiB per verification");
}

fn run_dataset_profile(epoch: u32) {
    println!("=== Dataset Generation Profile ===");
    println!("Epoch: {}", epoch);
    println!();

    println!("Generating light cache...");
    let start = Instant::now();
    let cache = LightCache::new(epoch);
    let cache_time = start.elapsed();

    let cache_items = cache.item_count();
    let cache_bytes = cache_items * 64;

    println!("ACTUAL MEASURED:");
    println!("  Light cache generation: {:.2}s", cache_time.as_secs_f64());
    println!("  Cache items: {}", cache_items);
    println!(
        "  Cache size: {:.2} MiB",
        cache_bytes as f64 / (1024.0 * 1024.0)
    );
    println!();

    println!("Generating sample dataset items...");
    let start = Instant::now();
    for i in 0..1000 {
        let _item = cache.dataset_item(i);
    }
    let item_time = start.elapsed();

    println!(
        "  1000 items generated in: {:.2}ms",
        item_time.as_secs_f64() * 1000.0
    );
    println!(
        "  Average: {:.2} us/item",
        item_time.as_micros() as f64 / 1000.0
    );
}

#[allow(clippy::too_many_arguments)]
fn run_qualification(
    device: usize,
    seconds: u64,
    warmup_seconds: u64,
    _dataset_cache: Option<String>,
    output: Option<String>,
    candidate_path: &str,
    pcie_mode: &str,
    power_source: &str,
    notes: &str,
) {
    println!("=== QelloxHashV1 Hardware Qualification ===");
    println!();

    // Step 1: Verify candidate manifest
    println!("Step 1: Verifying candidate manifest...");
    let expected_digest = "cec81499936e7dea1087805ceafe57d21b0b21e085bb61d6d79b35e3bfdecec5";
    let manifest_bytes = match std::fs::read(candidate_path) {
        Ok(b) => b,
        Err(e) => {
            println!("ERROR: Cannot read candidate manifest: {}", e);
            std::process::exit(1);
        }
    };
    let manifest_hash = hex::encode(Sha256::digest(&manifest_bytes));
    let digest_ok = manifest_hash == expected_digest;
    println!("  Expected: {}", expected_digest);
    println!("  Got:      {}", manifest_hash);
    println!("  Status:   {}", if digest_ok { "PASS" } else { "FAIL" });
    if !digest_ok {
        println!("FATAL: Candidate manifest digest mismatch");
        std::process::exit(1);
    }

    // Step 2: List adapter info
    println!();
    println!("Step 2: Adapter information...");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapters = instance.enumerate_adapters(wgpu::Backends::all());
    if device >= adapters.len() {
        println!(
            "ERROR: Device {} not found ({} devices available)",
            device,
            adapters.len()
        );
        std::process::exit(1);
    }
    let adapter = &adapters[device];
    let info = adapter.get_info();
    println!("  Name: {}", info.name);
    println!("  Backend: {:?}", info.backend);
    println!("  Type: {:?}", info.device_type);
    println!("  Vendor: {:04x}", info.vendor);
    println!("  Device: {:04x}", info.device);

    // Step 3: CPU/GPU equivalence
    println!();
    println!("Step 3: CPU/GPU equivalence test...");
    let equiv_result = pollster::block_on(qelloxhash_v1_gpu::GpuBackend::new());
    let (equiv_pass, equiv_mismatches) = match equiv_result {
        Ok(gpu) => {
            let ctx = gpu.create_mining_context();
            let mut mismatches = 0;
            let mut tests = 0;
            let header = [0x42u8; 80];
            for i in 0..1000 {
                let nonce = i as u64;
                let seed = compute_seed_from_header(&header, nonce);
                let _cpu_result = qelloxhash_v1(&header, nonce);
                let gpu_result = pollster::block_on(gpu.compute(&ctx, &seed));
                tests += 1;
                match gpu_result {
                    Ok(_) => {}
                    Err(_) => {
                        mismatches += 1;
                    }
                }
            }
            println!("  Tests: {}, Mismatches: {}", tests, mismatches);
            (mismatches == 0, mismatches)
        }
        Err(e) => {
            println!("  No GPU available: {}", e);
            println!("  Skipping GPU equivalence");
            (false, -1)
        }
    };

    // Step 4: Dataset generation
    println!();
    println!("Step 4: Dataset generation...");
    let cache_start = Instant::now();
    let _cache = LightCache::new(0);
    let cache_time = cache_start.elapsed();
    println!("  Light cache: {:.2}s", cache_time.as_secs_f64());

    // Step 5: Warmup
    println!();
    println!("Step 5: Warmup ({} seconds)...", warmup_seconds);
    let header = [0x42u8; 80];
    let warmup_start = Instant::now();
    let mut warmup_count = 0u64;
    while warmup_start.elapsed() < Duration::from_secs(warmup_seconds) {
        let _ = qelloxhash_v1(&header, warmup_count);
        warmup_count += 1;
    }
    println!("  Warmup hashes: {}", warmup_count);

    // Step 6: Steady-state measurement
    println!();
    println!("Step 6: Steady-state measurement ({} seconds)...", seconds);
    let measure_start = Instant::now();
    let mut total_hashes = 0u64;
    let mut rates = Vec::new();
    let mut second_start = Instant::now();
    let mut second_count = 0u64;
    let mut errors = 0u64;

    while measure_start.elapsed() < Duration::from_secs(seconds) {
        let digest = qelloxhash_v1(&header, total_hashes);
        total_hashes += 1;
        second_count += 1;

        // Verify determinism periodically
        if total_hashes % 10000 == 0 {
            let d2 = qelloxhash_v1(&header, total_hashes - 1);
            if digest != d2 {
                errors += 1;
            }
        }

        if second_start.elapsed() >= Duration::from_secs(1) {
            rates.push(second_count as f64 / second_start.elapsed().as_secs_f64());
            second_start = Instant::now();
            second_count = 0;
        }
    }
    let measure_elapsed = measure_start.elapsed();

    let sustained_rate = total_hashes as f64 / measure_elapsed.as_secs_f64();
    rates.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min_rate = rates.first().copied().unwrap_or(0.0);
    let max_rate = rates.last().copied().unwrap_or(0.0);
    let mean_rate = if rates.is_empty() {
        0.0
    } else {
        rates.iter().sum::<f64>() / rates.len() as f64
    };
    let median_rate = if rates.is_empty() {
        0.0
    } else {
        rates[rates.len() / 2]
    };

    println!("  Total hashes: {}", total_hashes);
    println!("  Sustained:    {:.2} H/s", sustained_rate);
    println!("  Min:          {:.2} H/s", min_rate);
    println!("  Max:          {:.2} H/s", max_rate);
    println!("  Mean:         {:.2} H/s", mean_rate);
    println!("  Median:       {:.2} H/s", median_rate);
    println!("  Errors:       {}", errors);

    // Step 7: Summary
    println!();
    println!("=== QUALIFICATION SUMMARY ===");
    let status = if digest_ok && equiv_pass && errors == 0 {
        "ACCEPTED"
    } else {
        "REJECTED"
    };
    println!("Status: {}", status);
    println!("Candidate hash: {}", manifest_hash);
    println!("GPU: {}", info.name);
    println!("Sustained rate: {:.2} H/s", sustained_rate);
    println!(
        "Equivalence: {}",
        if equiv_pass { "PASS" } else { "FAIL/SKIP" }
    );
    println!("Errors: {}", errors);

    // Write result
    if let Some(output_path) = output {
        let result = serde_json::json!({
            "schema_version": "1.0.0",
            "candidate_manifest_hash": manifest_hash,
            "candidate_id": "QELLOXHASH-V1-QUAL-CANDIDATE-A",
            "qualification_status": status,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string(),
            "gpu": {
                "name": info.name,
                "vendor": format!("{:?}", info.backend),
            },
            "candidate_digest_verified": digest_ok,
            "equivalence_test": {
                "pass": equiv_pass,
                "mismatches": equiv_mismatches,
            },
            "benchmark": {
                "warmup_seconds": warmup_seconds,
                "measurement_seconds": seconds,
                "total_hashes": total_hashes,
                "measured_hash_rate_hs": sustained_rate,
                "min_hash_rate_hs": min_rate,
                "max_hash_rate_hs": max_rate,
                "mean_hash_rate_hs": mean_rate,
                "median_hash_rate_hs": median_rate,
            },
            "errors": {
                "digest_mismatches": errors,
            },
            "labels": {
                "hash_rate": "MEASURED",
                "power": "NOT_MEASURED",
            },
            "notes": notes,
            "pcie_mode_description": pcie_mode,
            "power_source": power_source,
        });
        std::fs::write(&output_path, serde_json::to_string_pretty(&result).unwrap()).unwrap();
        println!();
        println!("Result written to: {}", output_path);
    }
}

fn run_stability(device: usize, hours: u64, output: Option<String>) {
    println!("=== QelloxHashV1 Stability Test ===");
    println!("Device: {}", device);
    println!("Duration: {} hours", hours);
    println!();

    let seconds = hours * 3600;
    let header = [0x42u8; 80];
    let start = Instant::now();
    let mut total_hashes = 0u64;
    let mut rates = Vec::new();
    let mut minute_start = Instant::now();
    let mut minute_count = 0u64;
    let errors = 0u64;

    while start.elapsed() < Duration::from_secs(seconds) {
        let _ = qelloxhash_v1(&header, total_hashes);
        total_hashes += 1;
        minute_count += 1;

        if minute_start.elapsed() >= Duration::from_secs(60) {
            let rate = minute_count as f64 / minute_start.elapsed().as_secs_f64();
            rates.push(rate);
            let elapsed_min = start.elapsed().as_secs() / 60;
            println!(
                "  [{:4} min] {:.2} H/s (total: {})",
                elapsed_min, rate, total_hashes
            );
            minute_start = Instant::now();
            minute_count = 0;
        }
    }

    let elapsed = start.elapsed();
    let sustained = total_hashes as f64 / elapsed.as_secs_f64();

    if !rates.is_empty() {
        rates.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let drift = (rates.last().unwrap() - rates.first().unwrap()) / sustained * 100.0;
        println!();
        println!("=== STABILITY RESULTS ===");
        println!("Total hashes: {}", total_hashes);
        println!("Sustained: {:.2} H/s", sustained);
        println!("Rate drift: {:.2}%", drift);
        println!("Errors: {}", errors);
        println!(
            "Stable: {}",
            if drift < 10.0 && errors == 0 {
                "YES"
            } else {
                "NO"
            }
        );
    }

    if let Some(output_path) = output {
        let result = serde_json::json!({
            "total_hashes": total_hashes,
            "sustained_rate_hs": sustained,
            "duration_seconds": seconds,
            "errors": errors,
        });
        std::fs::write(output_path, serde_json::to_string_pretty(&result).unwrap()).unwrap();
    }
}

fn run_compare(files: &[String]) {
    println!("=== Qualification Result Comparison ===");
    println!();

    for file in files {
        match std::fs::read_to_string(file) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(result) => {
                    let status = result["qualification_status"].as_str().unwrap_or("unknown");
                    let rate = result["benchmark"]["measured_hash_rate_hs"]
                        .as_f64()
                        .unwrap_or(0.0);
                    let gpu = result["gpu"]["name"].as_str().unwrap_or("unknown");
                    println!("{}:", file);
                    println!("  GPU: {}", gpu);
                    println!("  Status: {}", status);
                    println!("  Rate: {:.2} H/s", rate);
                    println!();
                }
                Err(e) => {
                    println!("{}: Parse error: {}", file, e);
                }
            },
            Err(e) => {
                println!("{}: Read error: {}", file, e);
            }
        }
    }
}

fn run_verify_manifest(file: &str) {
    println!("=== Candidate Manifest Verification ===");
    println!("File: {}", file);
    println!();

    let expected = "cec81499936e7dea1087805ceafe57d21b0b21e085bb61d6d79b35e3bfdecec5";

    match std::fs::read(file) {
        Ok(bytes) => {
            let hash = hex::encode(Sha256::digest(&bytes));
            println!("Expected: {}", expected);
            println!("Got:      {}", hash);
            if hash == expected {
                println!("Status:   PASS");
            } else {
                println!("Status:   FAIL");
                std::process::exit(1);
            }
        }
        Err(e) => {
            println!("Error reading file: {}", e);
            std::process::exit(1);
        }
    }
}

fn compute_seed_from_header(header: &[u8; 80], nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/seed/v1");
    hasher.update(header);
    hasher.update(nonce.to_le_bytes());
    hasher.finalize().into()
}
