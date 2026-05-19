# ColdVox Development Commands
# Install just: https://github.com/casey/just

set windows-powershell := true

# Default recipe lists all available commands
default:
    @just --list

# Run local CI checks (mirrors GitHub Actions exactly)
ci:
    bash ./scripts/local_ci.sh

# Run pre-commit hooks manually
check:
    pre-commit run --all-files

# Quick development checks (format, clippy, check)
lint:
    cargo fmt --all
    cargo clippy --all-targets --locked -- -D warnings
    cargo check --workspace --all-targets --locked

# Run all tests
test:
    {{ if os_family() == "windows" { "just windows-test" } else { "cargo test --workspace --locked" } }}

# Build all crates
build:
    cargo build --workspace --locked

# Build release
build-release:
    cargo build --workspace --locked --release

# Clean build artifacts
clean:
    cargo clean

# Format code
fmt:
    cargo fmt --all

# Generate documentation
docs:
    cargo doc --workspace --no-deps --locked --open

# Install pre-commit hooks
setup-hooks:
    pre-commit install

# Skip Rust checks in pre-commit (useful for quick commits)
commit-fast *args:
    {{ if os_family() == "windows" { "cmd /c \"set SKIP_RUST_CHECKS=1&& git commit " + args + "\"" } else { "SKIP_RUST_CHECKS=1 git commit " + args } }}

# Run specific test by name
test-filter filter:
    cargo test --workspace --locked {{filter}}

# Windows entrypoints for local run validation
windows-run-preflight:
    pwsh -NoProfile -File scripts/windows_live_validate.ps1 -Mode Preflight

windows-smoke:
    pwsh -NoProfile -File scripts/windows_live_validate.ps1 -Mode Smoke

windows-live:
    pwsh -NoProfile -File scripts/windows_live_validate.ps1 -Mode Live

# Canonical Parakeet HTTP container lifecycle for local/live STT development
parakeet-up:
    pwsh -NoProfile -File scripts/parakeet_http.ps1 -Action Up

parakeet-down:
    pwsh -NoProfile -File scripts/parakeet_http.ps1 -Action Down

parakeet-health:
    pwsh -NoProfile -File scripts/parakeet_http.ps1 -Action Health

parakeet-logs tail="200":
    pwsh -NoProfile -File scripts/parakeet_http.ps1 -Action Logs -Tail {{tail}}

parakeet-validate:
    pwsh -NoProfile -File scripts/integration_parakeet.ps1

# Live integration tests against the parakeet-cpu HTTP container.
# Brings the container up via docker compose, waits for /health, then runs
# both the plugin-level and the app-level wiring tests with -- --ignored.
integration-parakeet:
    pwsh -NoProfile -File scripts/integration_parakeet.ps1

# Windows-local test gate. Keep the required matrix package-scoped so it stays
# meaningful on Windows even while the wider workspace still includes
# non-Windows members.
windows-test:
    cargo test -p coldvox-foundation --lib --locked
    cargo test -p coldvox-audio --lib --locked
    cargo test -p coldvox-vad --lib --locked
    cargo test -p coldvox-telemetry --lib --locked
    cargo test -p coldvox-stt --lib --no-default-features --features http-remote --locked
    cargo test -p coldvox-gui --lib --locked
    cargo test -p coldvox-text-injection --lib --locked
    cargo test -p coldvox-text-injection --example test_enigo_live --no-run --no-default-features --features enigo --locked
    cargo test -p coldvox-app --lib --features http-remote --locked
    cargo test -p coldvox-app --test settings_test --locked
    cargo test -p coldvox-app --test verify_mock_injection_fix --locked
    cargo test -p coldvox-app --test golden_master --no-run --features http-remote,text-injection-enigo --locked
    just windows-smoke
    if ($env:COLDVOX_RUN_WINDOWS_LIVE -eq '1') { cargo run -p coldvox-text-injection --example test_enigo_live --no-default-features --features enigo --locked; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; just windows-live; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } } else { Write-Host 'Skipping live Windows validation; set COLDVOX_RUN_WINDOWS_LIVE=1 to run the Enigo example and just windows-live.' -ForegroundColor Yellow }

# Run main app with the canonical Windows HTTP remote profile
run:
    #!/usr/bin/env pwsh
    if ($IsWindows) {
        pwsh -NoProfile -File scripts/run-coldvox.ps1
        exit $LASTEXITCODE
    }
    cargo run -p coldvox-app --bin coldvox --features http-remote,text-injection

# Run TUI dashboard with the canonical Windows HTTP remote profile
tui:
    #!/usr/bin/env pwsh
    if ($IsWindows) {
        $base = uv run python -c "import sys; print(sys.base_prefix)"
        $env:PATH = "$base;$env:PATH"
    }
    cargo run -p coldvox-app --bin tui_dashboard --features http-remote,text-injection

# Run mic probe utility
mic-probe duration="30" command="mic-capture":
    cargo run -p coldvox-app --bin mic_probe -- {{command}} --duration {{duration}}
