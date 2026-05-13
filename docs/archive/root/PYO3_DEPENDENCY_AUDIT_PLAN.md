---
doc_type: plan
subsystem: dependencies
status: archived
last_reviewed: 2026-05-13
freshness: historical
preservation: preserve
summary: "Archived PyO3 dependency audit plan."
signals: ['pyo3', 'dependencies', 'archive']
---
# PyO3 Dependency Audit Plan for ColdVox

## Overview

This document provides a comprehensive, executable audit plan for diagnosing and resolving `DLL_NOT_FOUND` errors in the ColdVox project's PyO3 bindings. The plan is designed to be executed by an automated agent or developer.

**Target Environment:**
- **OS:** Windows 11 (primary), Linux/macOS (secondary)
- **Python:** 3.12 (managed by `uv`)
- **PyO3 Version:** 0.28
- **STT Backend:** Moonshine (PyO3/HuggingFace Transformers)

---

## Phase 1: Pre-Audit Environment Snapshot

### 1.1 Capture Python Environment

Execute these commands and record outputs:

```bash
# Python version and architecture
python --version
python -c "import sys; print(f'Executable: {sys.executable}'); print(f'Version: {sys.version}'); print(f'Architecture: {sys.maxsize > 2**32 and \"64-bit\" or \"32-bit\"}')"

# Virtual environment detection
python -c "import sys; print(f'Prefix: {sys.prefix}'); print(f'Base Prefix: {sys.base_prefix}'); print(f'In venv: {sys.prefix != sys.base_prefix}')"

# Environment variables
echo "PYTHONHOME: ${PYTHONHOME:-<unset>}"
echo "PYTHONPATH: ${PYTHONPATH:-<unset>}"
echo "PATH: $PATH"
```

### 1.2 Capture Python Packages

```bash
# Using uv (project standard)
uv pip list

# Fallback if uv unavailable
pip list

# Conda environment (if applicable)
conda list 2>/dev/null || echo "No conda environment detected"
```

### 1.3 Capture Rust/PyO3 Build Environment

```bash
# Rust toolchain
rustc --version
cargo --version

# PyO3 feature flags
grep -A 5 "pyo3" crates/coldvox-stt/Cargo.toml

# Check for compiled native libraries
find target -name "*.pyd" -o -name "*.so" -o -name "*.dylib" 2>/dev/null
```

### 1.4 Record System Information

```bash
# Windows
systeminfo | findstr /B /C:"OS Name" /C:"OS Version" /C:"System Type"

# Linux
uname -a
ldd --version

# macOS
sw_vers
otool -L 2>/dev/null | head -1
```

---

## Phase 2: Dependency Tree Analysis

### 2.1 Generate Python Dependency Tree

```bash
# Install pipdeptree if not present
uv pip install pipdeptree

# Generate full dependency tree
pipdeptree --warn silence > dependency_tree.txt

# Generate JSON format for parsing
pipdeptree --json > dependency_tree.json

# Identify packages with native extensions
pipdeptree --json | python -c "
import json, sys
tree = json.load(sys.stdin)
native_packages = []
for pkg in tree:
    if any(dep.get('installed_version', '').startswith(('cp', 'pp')) for dep in pkg.get('dependencies', [])):
        native_packages.append(pkg['package']['package_name'])
print('Packages with native extensions:', native_packages)
"
```

### 2.2 Identify PyO3/Rust Dependencies

```bash
# Check for PyO3 in Cargo.lock
grep -i "pyo3" Cargo.lock

# Check for Python bindings in Cargo.toml files
find crates -name "Cargo.toml" -exec grep -l "pyo3" {} \;

# List all Python packages that may have native code
python -c "
import pkg_resources
native_indicators = ['numpy', 'torch', 'scipy', 'librosa', 'transformers', 'cffi', 'pycparser']
for pkg in pkg_resources.working_set:
    if any(ind in pkg.project_name.lower() for ind in native_indicators):
        print(f'{pkg.project_name} {pkg.location}')
"
```

### 2.3 Map ColdVox Python Dependencies

From `pyproject.toml` and `uv.lock`, the critical Python dependencies are:

| Package | Version | Native? | Purpose |
|---------|---------|---------|---------|
| `transformers` | >=4.35.0 | Partial | HuggingFace model loading |
| `torch` | >=2.0.0 | **Yes** | PyTorch inference engine |
| `librosa` | >=0.10.0 | **Yes** | Audio processing |
| `numpy` | >=2.2.6 | **Yes** | Array operations |
| `scipy` | >=1.15.3 | **Yes** | Scientific computing |
| `cffi` | >=2.0.0 | **Yes** | C FFI bindings |

---

## Phase 3: DLL & Shared Library Mapping

### 3.1 Locate Native Libraries

#### Windows
```powershell
# Find all .dll files in Python environment
Get-ChildItem -Path (python -c "import sys; print(sys.prefix)") -Recurse -Filter "*.dll" | 
    Select-Object FullName, Length, LastWriteTime |
    Export-Csv -Path native_libs_windows.csv

# Find .pyd files (Python extension modules)
Get-ChildItem -Path (python -c "import sys; print(sys.prefix)") -Recurse -Filter "*.pyd" |
    Select-Object FullName |
    Export-Csv -Path python_extensions.csv
```

#### Linux
```bash
# Find all .so files
find $(python -c "import sys; print(sys.prefix)") -name "*.so" > native_libs_linux.txt

# Check for missing dependencies
ldd $(python -c "import sys; print(sys.executable)") | grep "not found"
```

#### macOS
```bash
# Find all .dylib files
find $(python -c "import sys; print(sys.prefix)") -name "*.dylib" > native_libs_macos.txt

# Check for missing dependencies
otool -L $(python -c "import sys; print(sys.executable)") | grep "not found"
```

### 3.2 Trace Native Library Dependencies

#### Windows (using dumpbin)
```powershell
# For each critical DLL, trace its dependencies
$critical_dlls = @(
    "torch*.dll",
    "numpy*.dll", 
    "scipy*.dll",
    "librosa*.dll"
)

foreach ($pattern in $critical_dlls) {
    Get-ChildItem -Path (python -c "import sys; print(sys.prefix)") -Recurse -Filter $pattern |
        ForEach-Object {
            Write-Host "=== $($_.Name) ==="
            dumpbin /dependents $_.FullName
        }
}
```

#### Linux (using ldd)
```bash
# Trace dependencies for critical .so files
for lib in $(find $(python -c "import sys; print(sys.prefix)") -name "*torch*.so" -o -name "*numpy*.so"); do
    echo "=== $(basename $lib) ==="
    ldd "$lib" | grep "not found"
done
```

#### macOS (using otool)
```bash
# Trace dependencies for critical .dylib files
for lib in $(find $(python -c "import sys; print(sys.prefix)") -name "*torch*.dylib" -o -name "*numpy*.dylib"); do
    echo "=== $(basename $lib) ==="
    otool -L "$lib" | grep "not found"
done
```

### 3.3 Check Rust Build Output

```bash
# Verify PyO3 compiled library exists
find target -name "coldvox_stt*.pyd" -o -name "coldvox_stt*.so" -o -name "coldvox_stt*.dylib"

# Check build metadata
ls -la target/*/build/coldvox-stt-*/output 2>/dev/null || echo "No build output found"

# Verify Python version compatibility
python -c "
import sys
print(f'Python {sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')
print(f'ABI: {sys.abiflags if hasattr(sys, \"abiflags\") else \"N/A\"}')
"
```

---

## Phase 4: PyO3/Python Environment Specific Checks

### 4.1 Verify Python Interpreter Consistency

```python
# Save as check_python_env.py
import sys
import os

print("=== Python Interpreter Verification ===")
print(f"Executable: {sys.executable}")
print(f"Version: {sys.version}")
print(f"Version Info: {sys.version_info}")
print(f"Platform: {sys.platform}")
print(f"Architecture: {sys.maxsize > 2**32 and '64-bit' or '32-bit'}")

print("\n=== Environment Variables ===")
print(f"PYTHONHOME: {os.environ.get('PYTHONHOME', '<unset>')}")
print(f"PYTHONPATH: {os.environ.get('PYTHONPATH', '<unset>')}")
print(f"VIRTUAL_ENV: {os.environ.get('VIRTUAL_ENV', '<unset>')}")
print(f"CONDA_PREFIX: {os.environ.get('CONDA_PREFIX', '<unset>')}")

print("\n=== Path Analysis ===")
python_exe = sys.executable
python_prefix = sys.prefix
print(f"Python executable location: {python_exe}")
print(f"Python prefix: {python_prefix}")
print(f"Expected venv path: {os.path.join(python_prefix, 'Scripts', 'python.exe') if sys.platform == 'win32' else os.path.join(python_prefix, 'bin', 'python')}")

# Check for multiple Python installations
print("\n=== Python Installation Check ===")
import subprocess
result = subprocess.run(['where', 'python'] if sys.platform == 'win32' else ['which', '-a', 'python'], 
                       capture_output=True, text=True)
print(f"All Python executables in PATH:\n{result.stdout}")
```

### 4.2 Check PYTHONHOME and PYTHONPATH

```bash
# Verify PYTHONHOME is unset (recommended for PyO3)
if [ -n "$PYTHONHOME" ]; then
    echo "WARNING: PYTHONHOME is set to: $PYTHONHOME"
    echo "PyO3 may fail to initialize Python correctly."
    echo "Recommendation: unset PYTHONHOME"
fi

# Check PYTHONPATH for conflicts
if [ -n "$PYTHONPATH" ]; then
    echo "PYTHONPATH is set to: $PYTHONPATH"
    echo "Checking for conflicts..."
    python -c "
import sys
import os
for path in sys.path:
    if path in os.environ.get('PYTHONPATH', '').split(os.pathsep):
        print(f'  Conflict: {path}')
"
fi
```

### 4.3 Detect Multiple Python Installations

```bash
# Windows
where python 2>nul
where python3 2>nul
reg query "HKLM\SOFTWARE\Python" /s 2>nul | findstr "InstallPath"
reg query "HKCU\SOFTWARE\Python" /s 2>nul | findstr "InstallPath"

# Linux
which -a python python3
ls -la /usr/bin/python* /usr/local/bin/python*

# macOS
which -a python python3
ls -la /usr/bin/python* /usr/local/bin/python* /opt/homebrew/bin/python*
```

### 4.4 Inspect Rust Build Directory

```bash
# Check for compiled PyO3 module
echo "=== Rust Build Output ==="
find target -name "*.pyd" -o -name "*.so" -o -name "*.dylib" | while read lib; do
    echo "Found: $lib"
    file "$lib" 2>/dev/null || echo "  (file command not available)"
done

# Check build script output
echo "=== Build Script Output ==="
find target -path "*/build/coldvox-stt-*/output" -exec cat {} \; 2>/dev/null || echo "No build output found"

# Verify Python version used during build
echo "=== Python Version Used in Build ==="
grep -r "python" target/*/build/coldvox-stt-*/output 2>/dev/null || echo "No Python version info found"
```

---

## Phase 5: Troubleshooting DLL_NOT_FOUND

### 5.1 Process Monitoring (Windows)

#### Using Process Monitor (ProcMon)
1. Download Process Monitor from Microsoft Sysinternals
2. Launch ProcMon as Administrator
3. Set filters:
   - Process Name: `python.exe`
   - Operation: `CreateFile`
   - Result: `NAME NOT FOUND` or `PATH NOT FOUND`
4. Run the ColdVox application with Moonshine feature
5. Capture events and filter for `.dll` and `.pyd` files
6. Export filtered events to CSV for analysis

#### Using Process Monitor Command Line
```powershell
# Start ProcMon logging (requires ProcMon in PATH)
procmon /BackingFile audit.pml /Quiet /AcceptEula

# Run ColdVox with Moonshine
cargo run -p coldvox-app --features moonshine

# Stop ProcMon and export
procmon /Terminate
procmon /OpenLog audit.pml /SaveAs audit.csv
```

### 5.2 File System Tracing (Linux)

```bash
# Using strace to trace file system access
strace -f -e trace=open,openat -o strace_output.txt cargo run -p coldvox-app --features moonshine

# Analyze strace output for missing files
grep "ENOENT" strace_output.txt | grep -E "\.(so|dll|pyd)" | head -20

# Using ltrace for library calls
ltrace -e "dlopen,dlsym" -o ltrace_output.txt cargo run -p coldvox-app --features moonshine
```

### 5.3 Check System PATH

```bash
# Windows
echo %PATH% | tr ';' '\n' | grep -i -E "(python|torch|cuda|msvc)"

# Linux
echo $PATH | tr ':' '\n' | grep -i -E "(python|torch|cuda)"

# macOS
echo $PATH | tr ':' '\n' | grep -i -E "(python|torch|cuda)"
```

### 5.4 Validate Visual C++ Redistributables (Windows)

```powershell
# Check installed VC++ Redistributables
Get-WmiObject -Class Win32_Product | Where-Object {$_.Name -like "*Visual C++*"} | 
    Select-Object Name, Version | Format-Table

# Check for required VC++ runtime DLLs
$vc_dlls = @("msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll")
foreach ($dll in $vc_dlls) {
    $found = Get-ChildItem -Path "C:\Windows\System32", "C:\Windows\SysWOW64" -Filter $dll -ErrorAction SilentlyContinue
    if ($found) {
        Write-Host "Found: $dll at $($found.FullName)"
    } else {
        Write-Host "MISSING: $dll"
    }
}
```

### 5.5 Validate Transitive Dependencies

```python
# Save as check_dependencies.py
import sys
import importlib
import pkg_resources

def check_package(package_name):
    """Check if a package and its dependencies are available."""
    try:
        pkg = pkg_resources.get_distribution(package_name)
        print(f"✓ {package_name} {pkg.version}")
        
        # Check for native extensions
        try:
            module = importlib.import_module(package_name.replace('-', '_'))
            if hasattr(module, '__file__'):
                print(f"  Location: {module.__file__}")
        except ImportError as e:
            print(f"  ✗ Import failed: {e}")
            
        return True
    except pkg_resources.DistributionNotFound:
        print(f"✗ {package_name} NOT FOUND")
        return False

# Check critical packages
critical_packages = [
    'torch',
    'transformers', 
    'librosa',
    'numpy',
    'scipy',
    'cffi',
    'pycparser'
]

print("=== Critical Package Check ===")
for pkg in critical_packages:
    check_package(pkg)

# Check PyO3 initialization
print("\n=== PyO3 Initialization Check ===")
try:
    import pyo3
    print(f"✓ PyO3 available")
except ImportError:
    print("✗ PyO3 not available (expected - it's a Rust crate)")

# Check if we can import torch (critical for Moonshine)
print("\n=== Torch Import Check ===")
try:
    import torch
    print(f"✓ PyTorch {torch.__version__}")
    print(f"  CUDA available: {torch.cuda.is_available()}")
    print(f"  CUDA version: {torch.version.cuda if torch.cuda.is_available() else 'N/A'}")
except ImportError as e:
    print(f"✗ PyTorch import failed: {e}")
```

---

## Phase 6: Output Deliverable

### 6.1 Structured Report Template

Generate a report with the following sections:

```markdown
# ColdVox PyO3 Dependency Audit Report

**Date:** [DATE]
**Auditor:** [AGENT_NAME]
**Environment:** [OS] [VERSION]

## Executive Summary
[Brief summary of findings and critical issues]

## 1. Environment Snapshot
- Python Version: [VERSION]
- Architecture: [32/64-bit]
- Virtual Environment: [YES/NO - PATH]
- Rust Toolchain: [VERSION]

## 2. Dependency Tree
### Direct Dependencies
| Package | Version | Status | Notes |
|---------|---------|--------|-------|
| [NAME] | [VER] | [OK/MISSING/MISMATCH] | [NOTES] |

### Transitive Dependencies
[Tree or list of transitive dependencies]

## 3. Native Libraries
### Python Extensions (.pyd/.so/.dylib)
| Library | Location | Dependencies | Status |
|---------|----------|--------------|--------|
| [NAME] | [PATH] | [DEPS] | [OK/MISSING] |

### Rust/PyO3 Libraries
| Library | Location | Python Version | Status |
|---------|----------|----------------|--------|
| [NAME] | [PATH] | [VER] | [OK/MISSING] |

## 4. DLL_NOT_FOUND Analysis
### Missing Libraries
| Library | Required By | Impact |
|---------|-------------|--------|
| [NAME] | [PACKAGE] | [CRITICAL/WARNING] |

### Dependency Chain
[Diagram or list showing dependency chains with missing links]

## 5. Environment Issues
### Python Interpreter
- [ ] Consistent interpreter used for build and runtime
- [ ] PYTHONHOME correctly set/unset
- [ ] PYTHONPATH correctly set/unset
- [ ] No conflicting Python installations

### System Dependencies
- [ ] Visual C++ Redistributables installed (Windows)
- [ ] Required system libraries available
- [ ] CUDA libraries available (if using GPU)

## 6. Corrective Actions
### Priority 1: Critical (Blocks functionality)
1. [ACTION]: [DESCRIPTION]
   - Command: `[COMMAND]`
   - Expected Result: [RESULT]

### Priority 2: Important (May cause issues)
1. [ACTION]: [DESCRIPTION]
   - Command: `[COMMAND]`
   - Expected Result: [RESULT]

### Priority 3: Recommended (Best practices)
1. [ACTION]: [DESCRIPTION]
   - Command: `[COMMAND]`
   - Expected Result: [RESULT]

## 7. Verification Steps
After applying corrective actions, verify with:
```bash
[VERIFICATION_COMMANDS]
```

## 8. Appendix
### A. Full Dependency Tree
[Paste output of `pipdeptree`]

### B. Native Library Locations
[Paste output of find/dir commands]

### C. Process Monitor/strace Output
[Relevant excerpts showing DLL loading failures]
```

### 6.2 Common Corrective Actions

Based on typical PyO3 DLL_NOT_FOUND issues:

#### Issue: Missing Visual C++ Redistributables
```powershell
# Download and install VC++ Redistributable
winget install Microsoft.VCRedist.2015+.x64
# Or download from: https://aka.ms/vs/17/release/vc_redist.x64.exe
```

#### Issue: Python Version Mismatch
```bash
# Rebuild with correct Python version
cargo clean -p coldvox-stt
cargo build -p coldvox-stt --features moonshine

# Verify Python version
python -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')"
```

#### Issue: Missing Python Dependencies
```bash
# Reinstall Python dependencies
uv sync --reinstall

# Or manually
uv pip install --force-reinstall torch transformers librosa
```

#### Issue: PYTHONHOME Set Incorrectly
```bash
# Unset PYTHONHOME (recommended for PyO3)
unset PYTHONHOME  # Linux/macOS
set PYTHONHOME=   # Windows (cmd)
$env:PYTHONHOME = $null  # Windows (PowerShell)
```

#### Issue: Multiple Python Installations
```bash
# Use uv to manage Python
uv python install 3.12
uv python pin 3.12

# Verify correct Python is used
uv run python -c "import sys; print(sys.executable)"
```

#### Issue: Missing CUDA Libraries (if using GPU)
```bash
# Install CUDA toolkit
# Windows: Download from NVIDIA
# Linux:
sudo apt-get install cuda-toolkit-12-1

# Verify CUDA
nvcc --version
nvidia-smi
```

---

## Execution Checklist

Use this checklist to track audit progress:

- [ ] Phase 1: Environment snapshot captured
- [ ] Phase 2: Dependency tree generated
- [ ] Phase 3: Native libraries mapped
- [ ] Phase 4: PyO3 environment verified
- [ ] Phase 5: DLL_NOT_FOUND troubleshooting completed
- [ ] Phase 6: Report generated with corrective actions
- [ ] Corrective actions applied
- [ ] Verification tests passed

---

## Notes

- This plan assumes Windows 11 as the primary target OS
- Linux and macOS commands are provided for cross-platform compatibility
- The `uv` package manager is the standard for this project (see `pyproject.toml`)
- PyO3 requires Python 3.10-3.12 (see `pyproject.toml` constraint)
- The Moonshine STT backend is the current working implementation
- Parakeet is planned but not yet functional

---

## References

- [PyO3 Documentation](https://pyo3.rs/)
- [PyO3 Troubleshooting](https://pyo3.rs/v0.20.3/troubleshooting)
- [ColdVox Architecture](docs/architecture.md)
- [ColdVox STT Overview](docs/domains/stt/stt-overview.md)
- [Process Monitor](https://learn.microsoft.com/en-us/sysinternals/downloads/procmon)
