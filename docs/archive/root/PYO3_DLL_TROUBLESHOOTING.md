---
doc_type: troubleshooting
subsystem: dependencies
status: archived
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Archived troubleshooting guide for PyO3 DLL loading issues."
signals: ['pyo3', 'dll', 'troubleshooting', 'archive']
---
# PyO3 DLL_NOT_FOUND Troubleshooting Guide

Quick reference for diagnosing and resolving DLL loading issues in ColdVox's PyO3 bindings.

---

## Quick Diagnosis Checklist

- [ ] Python version matches (3.10-3.12)
- [ ] PYTHONHOME is unset
- [ ] Visual C++ Redistributables installed (Windows)
- [ ] All Python dependencies installed via `uv sync`
- [ ] Rust build completed successfully
- [ ] No conflicting Python installations

---

## Common Issues and Solutions

### 1. Python Version Mismatch

**Symptoms:**
- `ImportError: DLL load failed while importing`
- `Python version mismatch`
- PyO3 initialization fails

**Diagnosis:**
```bash
# Check Python version used
python -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')"

# Check .python-version file
cat .python-version

# Verify uv is using correct Python
uv python list
```

**Solution:**
```bash
# Install correct Python version
uv python install 3.12

# Pin to correct version
uv python pin 3.12

# Rebuild Rust crate
cargo clean -p coldvox-stt
cargo build -p coldvox-stt --features moonshine
```

---

### 2. PYTHONHOME Set Incorrectly

**Symptoms:**
- `Py_Initialize: unable to load the file system codec`
- `Fatal Python error: initfsencoding`
- PyO3 fails to initialize

**Diagnosis:**
```bash
# Check if PYTHONHOME is set
echo $PYTHONHOME  # Linux/macOS
echo %PYTHONHOME%  # Windows (cmd)
echo $env:PYTHONHOME  # Windows (PowerShell)
```

**Solution:**
```bash
# Unset PYTHONHOME (Linux/macOS)
unset PYTHONHOME

# Unset PYTHONHOME (Windows cmd)
set PYTHONHOME=

# Unset PYTHONHOME (Windows PowerShell)
$env:PYTHONHOME = $null

# Make permanent: Remove from system/user environment variables
```

---

### 3. Missing Visual C++ Redistributables (Windows)

**Symptoms:**
- `The code execution cannot proceed because MSVCP140.dll was not found`
- `The code execution cannot proceed because VCRUNTIME140.dll was not found`
- `The code execution cannot proceed because VCRUNTIME140_1.dll was not found`

**Diagnosis:**
```powershell
# Check installed VC++ Redistributables
Get-WmiObject -Class Win32_Product | Where-Object {$_.Name -like "*Visual C++*"}

# Check for required DLLs
Get-ChildItem "C:\Windows\System32\msvcp140.dll" -ErrorAction SilentlyContinue
Get-ChildItem "C:\Windows\System32\vcruntime140.dll" -ErrorAction SilentlyContinue
Get-ChildItem "C:\Windows\System32\vcruntime140_1.dll" -ErrorAction SilentlyContinue
```

**Solution:**
```powershell
# Install via winget
winget install Microsoft.VCRedist.2015+.x64

# Or download from Microsoft
# https://aka.ms/vs/17/release/vc_redist.x64.exe

# Restart terminal after installation
```

---

### 4. Missing Python Dependencies

**Symptoms:**
- `ModuleNotFoundError: No module named 'torch'`
- `ModuleNotFoundError: No module named 'transformers'`
- `ImportError: cannot import name 'X' from 'torch'`

**Diagnosis:**
```bash
# Check installed packages
uv pip list

# Check specific package
uv pip show torch
uv pip show transformers
```

**Solution:**
```bash
# Reinstall all dependencies
uv sync

# Or reinstall specific package
uv pip install --force-reinstall torch>=2.0.0
uv pip install --force-reinstall transformers>=4.35.0

# Clear cache if needed
uv cache clean
```

---

### 5. Multiple Python Installations

**Symptoms:**
- Wrong Python version being used
- DLLs loaded from unexpected locations
- `ImportError` despite package being installed

**Diagnosis:**
```bash
# Find all Python installations
where python  # Windows
which -a python  # Linux/macOS

# Check which Python is being used
python -c "import sys; print(sys.executable)"

# Check PATH
echo $PATH | tr ':' '\n' | grep python
```

**Solution:**
```bash
# Use uv to manage Python
uv python install 3.12
uv python pin 3.12

# Verify correct Python
uv run python -c "import sys; print(sys.executable)"

# Remove conflicting Python from PATH if necessary
```

---

### 6. Missing CUDA Libraries (GPU Support)

**Symptoms:**
- `CUDA error: no kernel image is available for execution`
- `libcudart.so: cannot open shared object file`
- `torch.cuda.is_available()` returns `False`

**Diagnosis:**
```bash
# Check CUDA availability
python -c "import torch; print(f'CUDA available: {torch.cuda.is_available()}')"

# Check CUDA version
nvcc --version  # Linux
nvidia-smi

# Check CUDA libraries
ldconfig -p | grep cuda  # Linux
```

**Solution:**
```bash
# Install CUDA toolkit (Linux)
sudo apt-get install cuda-toolkit-12-1

# Install CUDA toolkit (Windows)
# Download from NVIDIA: https://developer.nvidia.com/cuda-downloads

# Reinstall PyTorch with CUDA support
uv pip install --force-reinstall torch --index-url https://download.pytorch.org/whl/cu121
```

---

### 7. Missing System Libraries (Linux)

**Symptoms:**
- `libpython3.12.so: cannot open shared object file`
- `error while loading shared libraries`

**Diagnosis:**
```bash
# Check for missing libraries
ldd $(which python) | grep "not found"

# Check library paths
ldconfig -p | grep python

# Check LD_LIBRARY_PATH
echo $LD_LIBRARY_PATH
```

**Solution:**
```bash
# Install Python development headers
sudo apt-get install python3.12-dev

# Update library cache
sudo ldconfig

# Set LD_LIBRARY_PATH if needed
export LD_LIBRARY_PATH=/usr/lib/python3.12/config:$LD_LIBRARY_PATH
```

---

### 8. Rust Build Artifacts Missing

**Symptoms:**
- `coldvox_stt` module not found
- `ImportError: dynamic module does not define module export function`
- PyO3 module not loading

**Diagnosis:**
```bash
# Check for compiled module
find target -name "*.pyd" -o -name "*.so" -o -name "*.dylib"

# Check build logs
ls -la target/*/build/coldvox-stt-*/output

# Verify feature flags
grep -A 5 "moonshine" crates/coldvox-stt/Cargo.toml
```

**Solution:**
```bash
# Clean and rebuild
cargo clean -p coldvox-stt
cargo build -p coldvox-stt --features moonshine

# Verify module exists
find target -name "coldvox_stt*"

# Check Python can import it
uv run python -c "import sys; sys.path.insert(0, 'target/debug'); import coldvox_stt"
```

---

## Advanced Debugging

### Using Process Monitor (Windows)

1. Download [Process Monitor](https://learn.microsoft.com/en-us/sysinternals/downloads/procmon)
2. Launch as Administrator
3. Set filters:
   - Process Name: `python.exe`
   - Operation: `CreateFile`
   - Result: `NAME NOT FOUND`
4. Run ColdVox and capture events
5. Look for failed DLL loads

### Using strace (Linux)

```bash
# Trace file system access
strace -f -e trace=open,openat -o strace.txt cargo run -p coldvox-app --features moonshine

# Find missing files
grep "ENOENT" strace.txt | grep -E "\.(so|dll)"
```

### Using ltrace (Linux)

```bash
# Trace library calls
ltrace -e "dlopen,dlsym" -o ltrace.txt cargo run -p coldvox-app --features moonshine
```

---

## Verification Commands

After applying fixes, verify everything works:

```bash
# 1. Check Python environment
uv run python -c "import sys; print(f'Python {sys.version}')"

# 2. Check PyTorch
uv run python -c "import torch; print(f'PyTorch {torch.__version__}'); print(f'CUDA: {torch.cuda.is_available()}')"

# 3. Check Transformers
uv run python -c "import transformers; print(f'Transformers {transformers.__version__}')"

# 4. Check Librosa
uv run python -c "import librosa; print(f'Librosa {librosa.__version__}')"

# 5. Check ColdVox STT
cargo check -p coldvox-stt --features moonshine

# 6. Run full test
cargo test -p coldvox-stt --features moonshine
```

---

## Getting Help

If issues persist:

1. Run the full audit: `./scripts/pyo3_audit.sh` or `.\scripts\pyo3_audit.ps1`
2. Review the generated `audit_report.md`
3. Check [PyO3 Troubleshooting](https://pyo3.rs/v0.20.3/troubleshooting)
4. Review ColdVox logs with `RUST_LOG=debug cargo run -p coldvox-app --features moonshine`

---

## References

- [PyO3 Documentation](https://pyo3.rs/)
- [PyO3 Troubleshooting](https://pyo3.rs/v0.20.3/troubleshooting)
- [PyTorch Installation](https://pytorch.org/get-started/locally/)
- [Visual C++ Redistributables](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
- [Process Monitor](https://learn.microsoft.com/en-us/sysinternals/downloads/procmon)
