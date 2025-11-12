# GPU Mock Configuration Guide

## Overview

The Capsuled Agent includes a **Hardware Abstraction Layer** that allows GPU-aware feature development and testing **without requiring actual GPU hardware**. This is achieved through a mock GPU detector that simulates NVIDIA GPU hardware.

## Architecture

### Two-Mode System

1. **Mock Mode** (Default)
   - Simulates GPU hardware using environment variables
   - No GPU or NVIDIA drivers required
   - Ideal for development, CI/CD, and testing
   - Always available and reliable

2. **Real Mode** (Optional)
   - Uses NVML (NVIDIA Management Library) for actual GPU detection
   - Requires `real-gpu` feature flag at compile time
   - Requires NVIDIA GPU and drivers installed
   - Automatically falls back to mock mode if NVML init fails

### Feature Flag

The `real-gpu` feature controls which mode is compiled:

```toml
# Cargo.toml
[features]
default = []
real-gpu = ["nvml-wrapper"]
```

## Mock Mode Configuration

### Environment Variables

Mock GPU behavior is controlled via environment variables:

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `MOCK_GPU_COUNT` | Number of simulated GPUs | `1` | `2` |
| `MOCK_VRAM_GB` | VRAM per GPU in gigabytes | `8` | `16` |
| `MOCK_CUDA_VERSION` | Simulated CUDA version | `"12.0"` | `"11.8"` |

### Configuration Examples

#### Single GPU (Default)
```bash
# Uses defaults: 1 GPU with 8 GB VRAM
cargo run
```

#### Dual GPU Workstation
```bash
export MOCK_GPU_COUNT=2
export MOCK_VRAM_GB=16
export MOCK_CUDA_VERSION="12.0"
cargo run
```

#### High-End Server (4x GPUs)
```bash
export MOCK_GPU_COUNT=4
export MOCK_VRAM_GB=24
export MOCK_CUDA_VERSION="12.1"
cargo run
```

#### CPU-Only Node
```bash
export MOCK_GPU_COUNT=0
cargo run
```

## Build Modes

### Default Build (Mock Only)

```bash
# Build without real GPU support
cargo build --release

# Run with custom mock config
MOCK_GPU_COUNT=2 MOCK_VRAM_GB=12 ./target/release/capsuled-engine
```

### Build with Real GPU Support

```bash
# Enable real-gpu feature (requires NVIDIA drivers)
cargo build --release --features real-gpu

# Will attempt NVML detection first, fallback to mock if failed
./target/release/capsuled-engine
```

## Integration Example

### In main.rs

```rust
use hardware::create_gpu_detector;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // GPU detection happens automatically
    let gpu_detector = create_gpu_detector();
    
    match gpu_detector.detect_gpus() {
        Ok(report) => {
            println!("Rig ID: {}", report.rig_id);
            println!("GPU Count: {}", report.gpu_count());
            println!("Total VRAM: {:.2} GB", report.total_vram_gb());
            
            if report.is_mock {
                println!("Running in MOCK mode");
            } else {
                println!("Running with REAL hardware");
            }
        }
        Err(e) => {
            eprintln!("GPU detection failed: {}", e);
        }
    }
    
    Ok(())
}
```

### Expected Output (Mock Mode)

```
[INFO] Capsuled Engine starting...
[INFO] Version: 0.1.0
[INFO] Detecting GPU hardware...
[INFO] Using mock GPU detector (real-gpu feature not enabled)
[INFO] Hardware detection completed:
[INFO]   Rig ID: my-laptop
[INFO]   GPU Count: 2
[INFO]   Total VRAM: 32.00 GB
[INFO]   Mode: Mock (set MOCK_GPU_COUNT, MOCK_VRAM_GB to configure)
[INFO]   CUDA Version: 12.0
[INFO]     GPU 0: Mock NVIDIA GPU 0 (16.00 GB VRAM)
[INFO]     GPU 1: Mock NVIDIA GPU 1 (16.00 GB VRAM)
```

## Testing

### Unit Tests

Unit tests are included in `gpu_detector.rs`:

```bash
# Run all hardware module tests
cargo test --package capsuled-engine --lib hardware

# Run specific mock detector tests
cargo test --package capsuled-engine --lib hardware::gpu_detector::tests
```

### Integration Testing

Create different scenarios for scheduler testing:

```bash
# Test single-GPU allocation
MOCK_GPU_COUNT=1 MOCK_VRAM_GB=8 cargo test test_single_gpu_scheduling

# Test multi-GPU load balancing
MOCK_GPU_COUNT=4 MOCK_VRAM_GB=16 cargo test test_multi_gpu_scheduling

# Test no-GPU fallback
MOCK_GPU_COUNT=0 cargo test test_cpu_only_scheduling
```

## CI/CD Configuration

### GitHub Actions Example

```yaml
name: GPU Tests

on: [push, pull_request]

jobs:
  test-gpu-scenarios:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        gpu_config:
          - { count: 0, vram: 0 }     # CPU-only
          - { count: 1, vram: 8 }     # Single GPU
          - { count: 2, vram: 16 }    # Dual GPU
          - { count: 4, vram: 24 }    # Multi GPU
    
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      
      - name: Run tests with GPU config
        env:
          MOCK_GPU_COUNT: ${{ matrix.gpu_config.count }}
          MOCK_VRAM_GB: ${{ matrix.gpu_config.vram }}
          MOCK_CUDA_VERSION: "12.0"
        run: cargo test --package capsuled-engine
```

## Docker Configuration

### Dockerfile

```dockerfile
FROM rust:1.75 AS builder

WORKDIR /app
COPY . .

# Build with mock support (default)
RUN cargo build --release

# Or build with real GPU support
# RUN cargo build --release --features real-gpu

FROM debian:bookworm-slim

# Copy binary
COPY --from=builder /app/target/release/capsuled-engine /usr/local/bin/

# Set default mock configuration
ENV MOCK_GPU_COUNT=1
ENV MOCK_VRAM_GB=8
ENV MOCK_CUDA_VERSION="12.0"

CMD ["capsuled-engine"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  agent-dual-gpu:
    image: capsuled-engine:latest
    environment:
      MOCK_GPU_COUNT: 2
      MOCK_VRAM_GB: 16
      MOCK_CUDA_VERSION: "12.0"
    ports:
      - "50051:50051"
  
  agent-cpu-only:
    image: capsuled-engine:latest
    environment:
      MOCK_GPU_COUNT: 0
    ports:
      - "50052:50051"
```

## Development Workflow

### Local Development

1. **Start with defaults** for basic testing:
   ```bash
   cargo run
   ```

2. **Test specific scenarios** with env vars:
   ```bash
   MOCK_GPU_COUNT=4 MOCK_VRAM_GB=24 cargo run
   ```

3. **Run unit tests** to validate changes:
   ```bash
   cargo test hardware
   ```

### Integration with Coordinator

When the Coordinator requests hardware reports from Agents:

1. Agent runs `gpu_detector.detect_gpus()`
2. Returns `RigHardwareReport` with mock or real data
3. Coordinator uses `report.is_mock` to handle appropriately
4. Scheduler treats mock reports identically to real reports

## Transition to Real Hardware

When deploying to nodes with actual GPUs:

### Option 1: Build-Time Switch

```bash
# Build with real GPU support
cargo build --release --features real-gpu

# Deploy to GPU nodes
scp target/release/capsuled-engine gpu-node:/usr/local/bin/
```

### Option 2: Runtime Fallback

The factory function automatically handles transitions:

```rust
pub fn create_gpu_detector() -> Arc<dyn GpuDetector> {
    #[cfg(feature = "real-gpu")]
    {
        match NvmlGpuDetector::new() {
            Ok(detector) => Arc::new(detector),      // Use real GPU
            Err(_) => Arc::new(MockGpuDetector::new()) // Fallback to mock
        }
    }
    
    #[cfg(not(feature = "real-gpu"))]
    {
        Arc::new(MockGpuDetector::new())  // Mock only
    }
}
```

### Verification

Check logs to confirm detection mode:

- **Mock**: `"Using mock GPU detector (real-gpu feature not enabled)"`
- **Real**: `"Using NVML GPU detector"`
- **Fallback**: `"NVML init failed, falling back to mock detector"`

## Troubleshooting

### Issue: "No GPUs detected"

**Mock Mode:**
- Check `MOCK_GPU_COUNT` is set and > 0
- Verify env vars are exported before running

**Real Mode:**
- Verify NVIDIA drivers installed: `nvidia-smi`
- Check `real-gpu` feature is enabled in build
- Review logs for NVML init errors

### Issue: "VRAM calculations incorrect"

**Mock Mode:**
- `MOCK_VRAM_GB` is per-GPU, not total
- Example: `MOCK_GPU_COUNT=2 MOCK_VRAM_GB=8` = 16 GB total

**Real Mode:**
- NVML reports actual hardware values
- No configuration needed

### Issue: "Tests fail in CI"

- Ensure env vars are set in CI config
- Use matrix strategy to test multiple scenarios
- Mock mode should always work in CI (no GPU required)

## Best Practices

1. **Default to Mock Mode** for development and CI/CD
2. **Use env vars** for different test scenarios
3. **Test CPU-only nodes** with `MOCK_GPU_COUNT=0`
4. **Enable real-gpu feature** only for production GPU nodes
5. **Log detection mode** for observability
6. **Handle mock vs real** in Coordinator for accurate metrics

## Future Enhancements

- [ ] AMD GPU support (via ROCm)
- [ ] Intel GPU support (via Level Zero)
- [ ] Mock GPU utilization simulation
- [ ] Mock power consumption data
- [ ] Dynamic GPU failure simulation for resilience testing

## References

- [NVML Documentation](https://docs.nvidia.com/deploy/nvml-api/)
- [nvml-wrapper Crate](https://docs.rs/nvml-wrapper/)
- [NVIDIA Container Toolkit](https://github.com/NVIDIA/nvidia-container-toolkit)
