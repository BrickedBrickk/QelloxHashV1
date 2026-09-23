use sha2::{Digest, Sha256};
use std::borrow::Cow;

// Import constants from reference implementation
const LANES: usize = 32;
const REGISTERS: usize = 32;
const SCRATCHPAD_WORDS: usize = 12288;
const DEFAULT_PROGRAM_LENGTH: usize = 112;

/// GPU backend for QelloxHashV1 computation.
pub struct GpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

/// GPU context for a single mining operation.
pub struct GpuMiningContext {
    instructions_buffer: wgpu::Buffer,
    cross_lane_map_buffer: wgpu::Buffer,
    registers_buffer: wgpu::Buffer,
    scratchpad_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// Result from GPU computation.
#[derive(Debug, Clone)]
pub struct GpuResult {
    pub work_digest: [u8; 32],
}

impl GpuBackend {
    /// Creates a new GPU backend.
    pub async fn new() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(GpuError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("QelloxHashV1 Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .map_err(|e| GpuError::DeviceCreation(e.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("QelloxHashV1 Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "../shaders/qelloxhash_v1.wgsl"
            ))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("QelloxHashV1 Bind Group Layout"),
            entries: &[
                // instructions - read only
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // cross_lane_map - read only
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // registers - read/write
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // scratchpad - read/write
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // output - write only
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("QelloxHashV1 Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("QelloxHashV1 Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "main",
            compilation_options: Default::default(),
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
        })
    }

    /// Creates a mining context with pre-allocated buffers.
    pub fn create_mining_context(&self) -> GpuMiningContext {
        let instructions_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instructions Buffer"),
            size: (DEFAULT_PROGRAM_LENGTH * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let cross_lane_map_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cross Lane Map Buffer"),
            size: (LANES * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let registers_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Registers Buffer"),
            size: (LANES * REGISTERS * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let scratchpad_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Scratchpad Buffer"),
            size: (SCRATCHPAD_WORDS * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: 32, // 8 u32 words = 256 bits
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Readback Buffer"),
            size: 32,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("QelloxHashV1 Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instructions_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cross_lane_map_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: registers_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: scratchpad_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        GpuMiningContext {
            instructions_buffer,
            cross_lane_map_buffer,
            registers_buffer,
            scratchpad_buffer,
            output_buffer,
            readback_buffer,
            bind_group,
        }
    }

    /// Computes QelloxHashV1 on the GPU.
    pub async fn compute(
        &self,
        ctx: &GpuMiningContext,
        seed: &[u8; 32],
    ) -> Result<GpuResult, GpuError> {
        // Generate program on CPU (program generation is fast and deterministic)
        let program = generate_program_gpu(seed);

        // Initialize registers on CPU
        let mut registers = vec![0u32; LANES * REGISTERS];
        initialize_registers(seed, &mut registers);

        // Initialize scratchpad on CPU
        let mut scratchpad = vec![0u32; SCRATCHPAD_WORDS];
        initialize_scratchpad(seed, &mut scratchpad);

        // Upload data to GPU
        self.queue.write_buffer(
            &ctx.instructions_buffer,
            0,
            bytemuck::cast_slice(&program.instructions),
        );
        self.queue.write_buffer(
            &ctx.cross_lane_map_buffer,
            0,
            bytemuck::cast_slice(&program.cross_lane_map),
        );
        self.queue
            .write_buffer(&ctx.registers_buffer, 0, bytemuck::cast_slice(&registers));
        self.queue
            .write_buffer(&ctx.scratchpad_buffer, 0, bytemuck::cast_slice(&scratchpad));

        // Dispatch compute shader
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("QelloxHashV1 Encoder"),
            });

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("QelloxHashV1 Compute Pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &ctx.bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
        }

        // Copy output to readback buffer
        encoder.copy_buffer_to_buffer(&ctx.output_buffer, 0, &ctx.readback_buffer, 0, 32);

        self.queue.submit(Some(encoder.finish()));

        // Read back result
        let buffer_slice = ctx.readback_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });

        self.device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .unwrap()
            .map_err(|e| GpuError::Readback(e.to_string()))?;

        let data = buffer_slice.get_mapped_range();
        let mut work_digest = [0u8; 32];
        work_digest.copy_from_slice(&data);
        drop(data);
        ctx.readback_buffer.unmap();

        Ok(GpuResult { work_digest })
    }

    /// Returns information about the GPU device.
    pub fn device_info(&self) -> GpuDeviceInfo {
        GpuDeviceInfo {
            name: "GPU".to_string(), // Would need adapter info
            backend: "wgpu".to_string(),
        }
    }
}

/// GPU device information.
pub struct GpuDeviceInfo {
    pub name: String,
    pub backend: String,
}

/// Errors that can occur during GPU operations.
#[derive(Debug)]
pub enum GpuError {
    NoAdapter,
    DeviceCreation(String),
    Readback(String),
    PipelineCreation(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuError::NoAdapter => write!(f, "No suitable GPU adapter found"),
            GpuError::DeviceCreation(msg) => write!(f, "Failed to create device: {}", msg),
            GpuError::Readback(msg) => write!(f, "Failed to read back result: {}", msg),
            GpuError::PipelineCreation(msg) => write!(f, "Failed to create pipeline: {}", msg),
        }
    }
}

impl std::error::Error for GpuError {}

/// Program representation for GPU.
struct GpuProgram {
    instructions: Vec<u32>,
    cross_lane_map: Vec<u32>,
}

/// Generates a program for GPU execution.
fn generate_program_gpu(seed: &[u8; 32]) -> GpuProgram {
    // Use the same algorithm as CPU reference
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/program-expand/v1");
    hasher.update(seed);
    hasher.update(0u32.to_le_bytes());
    let hash: [u8; 32] = hasher.finalize().into();

    let mut instructions = Vec::with_capacity(DEFAULT_PROGRAM_LENGTH);
    let mut cross_lane_map = Vec::with_capacity(LANES);

    // Generate instructions from seed expansion
    for i in 0..DEFAULT_PROGRAM_LENGTH {
        let offset = i * 4;
        let raw = u32::from_le_bytes([
            hash[offset % 32],
            hash[(offset + 1) % 32],
            hash[(offset + 2) % 32],
            hash[(offset + 3) % 32],
        ]);
        instructions.push(raw);
    }

    // Generate cross-lane map
    for i in 0..LANES {
        let val = hash[(DEFAULT_PROGRAM_LENGTH * 4 + i) % 32] as u32;
        let peer = if (val % LANES as u32) == i as u32 {
            (val + 1) % LANES as u32
        } else {
            val % LANES as u32
        };
        cross_lane_map.push(peer);
    }

    GpuProgram {
        instructions,
        cross_lane_map,
    }
}

/// Initializes registers from seed.
fn initialize_registers(seed: &[u8; 32], registers: &mut [u32]) {
    for lane in 0..LANES {
        for reg in 0..REGISTERS {
            let mut hasher = Sha256::new();
            hasher.update(b"QelloxHashV1/reg-init/v1");
            hasher.update(seed);
            hasher.update((lane as u32).to_le_bytes());
            hasher.update((reg as u32).to_le_bytes());
            let hash: [u8; 32] = hasher.finalize().into();
            registers[lane * REGISTERS + reg] =
                u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]);
        }
    }
}

/// Initializes scratchpad from seed.
fn initialize_scratchpad(seed: &[u8; 32], scratchpad: &mut [u32]) {
    let expand_blocks = SCRATCHPAD_WORDS.div_ceil(8);
    for i in 0..expand_blocks {
        let mut hasher = Sha256::new();
        hasher.update(b"QelloxHashV1/scratch-init/v1");
        hasher.update(seed);
        hasher.update((i as u32).to_le_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        for j in 0..8 {
            let idx = i * 8 + j;
            if idx < SCRATCHPAD_WORDS {
                scratchpad[idx] = u32::from_le_bytes([
                    hash[j * 4],
                    hash[j * 4 + 1],
                    hash[j * 4 + 2],
                    hash[j * 4 + 3],
                ]);
            }
        }
    }
}
