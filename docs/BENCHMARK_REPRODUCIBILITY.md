# Benchmark Reproducibility Guide

This guide ensures you can reproduce the benchmark results reported in `BENCHMARK_REPORT.md` with high fidelity.

## Quick Start

```bash
# 1. Clone repository
git clone https://github.com/yourorg/sqlitegraph.git
cd sqlitegraph

# 2. Run benchmarks
cargo bench --features native-v3

# 3. View HTML report
open target/criterion/report/index.html
```

## Detailed Setup

### 1. Hardware Requirements

**Minimum Recommended:**
- CPU: 4+ cores (x86_64 or ARM64)
- RAM: 8GB+
- Storage: SSD (NVMe preferred)
- OS: Linux kernel 5.0+ (Ubuntu 22.04 LTS tested)

**Tested Configuration (Reference):**
```
CPU: Intel Core i7-12700K (12 cores, 3.6 GHz base)
RAM: 32 GB DDR4-3200
Storage: Samsung 980 Pro NVMe SSD 1TB
OS: Ubuntu 24.04 LTS (Kernel 6.8.0)
```

### 2. Software Prerequisites

```bash
# Install Rust (latest stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verify installation
rustc --version  # Should be 1.75.0 or later
cargo --version

# Install cargo-criterion for HTML reports
cargo install cargo-criterion

# Install system dependencies (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install -y linux-tools-common linux-tools-generic
```

### 3. System Tuning for Stable Results

#### 3.1 CPU Governor

```bash
# Set CPU to performance mode
sudo cpupower frequency-set -g performance

# Verify
cpupower frequency-info
```

#### 3.2 Disable CPU Boost (Intel)

```bash
# Temporary (until reboot)
echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo

# Permanent (create systemd service)
sudo tee /etc/systemd/system/disable-turbo-boost.service << 'EOF'
[Unit]
Description=Disable CPU Turbo Boost
After=multi-user.target

[Service]
Type=oneshot
ExecStart=/bin/bash -c 'echo 1 > /sys/devices/system/cpu/intel_pstate/no_turbo'
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl enable disable-turbo-boost.service
```

#### 3.3 Isolate CPUs (Advanced)

For maximum stability, isolate specific CPUs for benchmarking:

```bash
# Edit /etc/default/grub
sudo sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="[^"]*"/GRUB_CMDLINE_LINUX_DEFAULT="quiet isolcpus=0-3"/' /etc/default/grub
sudo update-grub
sudo reboot

# Run benchmarks on isolated CPUs
taskset -c 0,1,2,3 cargo bench --features native-v3
```

#### 3.4 Disable Swap

```bash
# Check current swap
swapon --show

# Disable temporarily
sudo swapoff -a

# Re-enable after benchmarks
sudo swapon -a
```

#### 3.5 Stop Background Services

```bash
# Stop non-essential services
sudo systemctl stop cron
sudo systemctl stop snapd
sudo systemctl stop bluetooth
sudo systemctl stop cups
sudo systemctl stop NetworkManager  # Only if running locally

# Check for CPU usage
htop  # Verify no processes using >1% CPU
```

### 4. Running Benchmarks

#### 4.1 Standard Run

```bash
cd /path/to/sqlitegraph

# Clean previous results
rm -rf target/criterion

# Run all benchmarks
cargo bench --features native-v3 2>&1 | tee benchmark_output.log

# This will take ~30-60 minutes depending on hardware
```

#### 4.2 Specific Benchmark

```bash
# Run only BFS benchmarks
cargo bench --features native-v3 -- bfs_traversal

# Run only SQLite benchmarks
cargo bench --features native-v3 -- sqlite

# Run only specific size
cargo bench --features native-v3 -- "medium_random_10k"
```

#### 4.3 Statistical Verification

```bash
# Run 3 times and compare results
for i in 1 2 3; do
    echo "=== Run $i ==="
    rm -rf target/criterion
    cargo bench --features native-v3 2>&1 | tee "run_$i.log"
done

# Compare results (should be within 5% of each other)
```

### 5. Environment Validation

Run this script to verify your environment:

```bash
#!/bin/bash
# verify_env.sh

echo "=== Environment Validation ==="

# Check CPU governor
echo "CPU Governor:"
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor

# Check available memory
echo -e "\nMemory:"
free -h

# Check disk type
echo -e "\nDisk Type:"
cat /sys/block/nvme0n1/queue/rotational 2>/dev/null || echo "N/A"
lsblk -d -o NAME,ROTA,TYPE,SIZE,MODEL

# Check temperature (if available)
echo -e "\nTemperature:"
sensors 2>/dev/null || echo "lm-sensors not installed"

# Check for background processes
echo -e "\nTop CPU processes:"
ps aux --sort=-%cpu | head -10

# Check Rust version
echo -e "\nRust Version:"
rustc --version
cargo --version

echo -e "\n=== Validation Complete ==="
```

### 6. Understanding Results

#### 6.1 Console Output

```
bfs_traversal/sqlite/small_random_1k_5k
                        time:   [2.4512 ms 2.4589 ms 2.4678 ms]
                        thrpt:  [405.21 Kelem/s 406.68 Kelem/s 407.96 Kelem/s]
Found 10 outliers among 100 measurements (10.00%)
  4 (4.00%) high mild
  6 (6.00%) high severe
```

**Reading the output:**
- `time: [min mean max]` - 95% confidence interval
- `thrpt:` - Throughput in elements/second
- `outliers` - Measurements outside 1.5× IQR (these are excluded from final stats)

#### 6.2 HTML Report

```bash
# Open HTML report
open target/criterion/report/index.html  # macOS
xdg-open target/criterion/report/index.html  # Linux
```

The HTML report includes:
- ** violin plots:** Distribution of measurements
- **Line charts:** Performance over time (detects drift)
- **Comparison tables:** Side-by-side backend comparison

### 7. Troubleshooting

#### 7.1 High Variance

If you see high standard deviation (>10%):

```bash
# Check for thermal throttling
cat /sys/class/thermal/thermal_zone*/temp

# Check CPU frequency stability
watch -n 1 cat /proc/cpuinfo | grep MHz

# Ensure no background processes
sudo systemctl stop <service-name>
```

#### 7.2 Outliers

Many outliers indicate:
- **Cause:** OS scheduler interference, background processes
- **Fix:** Use CPU isolation (`taskset`), disable unnecessary services

#### 7.3 Slow Results

If your results are much slower than the report:

1. **Check build profile:**
   ```bash
   cargo bench --features native-v3 -- --profile-time 10
   # Should show "release" profile
   ```

2. **Verify features:**
   ```bash
   cargo tree --features native-v3  # Check native-v3 is enabled
   ```

3. **Check disk type:**
   ```bash
   # HDDs will be 10-100× slower for SQLite
   lsblk -d -o NAME,ROTA,TYPE
   ```

### 8. Reporting Issues

If you cannot reproduce results, please include:

1. **System info:**
   ```bash
   uname -a
   lscpu
   free -h
   lsblk
   ```

2. **Benchmark output:**
   ```bash
   cargo bench --features native-v3 -- --verbose 2>&1 | tee issue_output.log
   ```

3. **Environment validation:**
   ```bash
   ./verify_env.sh
   ```

### 9. Docker Reproduction

For completely reproducible environments:

```dockerfile
FROM rust:1.75-bookworm

RUN apt-get update && apt-get install -y \
    linux-perf \
    cpupower-gui \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /benchmark
COPY . .

# Set CPU governor in container (requires --privileged)
RUN echo '#!/bin/bash\ncpupower frequency-set -g performance\ncargo bench --features native-v3' > /benchmark/run.sh
RUN chmod +x /benchmark/run.sh

CMD ["/benchmark/run.sh"]
```

Run with:
```bash
docker build -t sqlitegraph-bench .
docker run --privileged --rm -v $(pwd)/results:/benchmark/target/criterion sqlitegraph-bench
```

### 10. CI/CD Integration

For automated benchmark tracking:

```yaml
# .github/workflows/benchmark.yml
name: Benchmark

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: dtolnay/rust-action@stable
    
    - name: Setup environment
      run: |
        sudo apt-get install -y linux-tools-common
        echo "performance" | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
    
    - name: Run benchmarks
      run: cargo bench --features native-v3
    
    - name: Upload results
      uses: actions/upload-artifact@v3
      with:
        name: benchmark-results
        path: target/criterion/
```

---

## References

- [Criterion.rs Book](https://bheisler.github.io/criterion.rs/book/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Linux Kernel Benchmarking](https://www.kernel.org/doc/html/latest/admin-guide/kernel-perf.html)

---

*Last updated: 2026-02-12*
