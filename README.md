# Broadcaster

A Windows live streaming application built with Rust and Tauri.

## Prerequisites

Before you begin, install these tools:

### 1. Install Rust

Open PowerShell and run:
```powershell
# Download and run the Rust installer
winget install Rustlang.Rustup

# Or visit https://rustup.rs and download rustup-init.exe
```

After installation, restart your terminal and verify:
```powershell
rustc --version    # Should show: rustc 1.77.0 or higher
cargo --version    # Should show: cargo 1.77.0 or higher
```

### 2. Install Node.js (for Tauri UI)

```powershell
winget install OpenJS.NodeJS.LTS

# Verify installation
node --version     # Should show: v20.x.x or higher
npm --version      # Should show: 10.x.x or higher
```

### 3. Install Visual Studio Build Tools

Rust on Windows requires MSVC build tools:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
```

Or download from: https://visualstudio.microsoft.com/visual-cpp-build-tools/

During installation, select:
- "Desktop development with C++"
- Windows 10/11 SDK

### 4. Install Tauri CLI

```powershell
cargo install tauri-cli
```

### 5. Install Native Dependencies (x264, fdk-aac)

The project requires native video/audio encoding libraries. Install them using vcpkg and Chocolatey:

**Install vcpkg (C++ package manager):**
```powershell
# Clone vcpkg to C:\vcpkg (recommended location)
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
cd C:\vcpkg

# Bootstrap vcpkg
.\bootstrap-vcpkg.bat
```

**Install native libraries via vcpkg:**
```powershell
C:\vcpkg\vcpkg install x264:x64-windows-static fdk-aac:x64-windows-static
```

This will also automatically install LLVM/Clang (required for `bindgen` to generate Rust bindings).

**Install pkg-config via Chocolatey:**
```powershell
# Install Chocolatey if you don't have it (run as Administrator)
# See https://chocolatey.org/install

# Install pkg-config-lite
choco install pkgconfiglite
```

### 6. Configure Environment Variables

The build system needs to know where to find the native libraries. Set these environment variables:

**Per-session (PowerShell):**
```powershell
$env:PKG_CONFIG_PATH = "C:\vcpkg\installed\x64-windows-static\lib\pkgconfig"
$env:X264_LIB_DIR = "C:\vcpkg\installed\x64-windows-static\lib"
$env:X264_INCLUDE_DIR = "C:\vcpkg\installed\x64-windows-static\include"
$env:FDK_AAC_LIB_DIR = "C:\vcpkg\installed\x64-windows-static\lib"
$env:FDK_AAC_INCLUDE_DIR = "C:\vcpkg\installed\x64-windows-static\include"
$env:LIBCLANG_PATH = "C:\vcpkg\installed\x64-windows-static\tools\llvm\bin"
```

**Permanent setup (run as Administrator):**
```powershell
[System.Environment]::SetEnvironmentVariable("PKG_CONFIG_PATH", "C:\vcpkg\installed\x64-windows-static\lib\pkgconfig", "User")
[System.Environment]::SetEnvironmentVariable("X264_LIB_DIR", "C:\vcpkg\installed\x64-windows-static\lib", "User")
[System.Environment]::SetEnvironmentVariable("X264_INCLUDE_DIR", "C:\vcpkg\installed\x64-windows-static\include", "User")
[System.Environment]::SetEnvironmentVariable("FDK_AAC_LIB_DIR", "C:\vcpkg\installed\x64-windows-static\lib", "User")
[System.Environment]::SetEnvironmentVariable("FDK_AAC_INCLUDE_DIR", "C:\vcpkg\installed\x64-windows-static\include", "User")
[System.Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\vcpkg\installed\x64-windows-static\tools\llvm\bin", "User")
```

After setting permanent variables, restart your terminal for changes to take effect.

### 7. (Optional) NVIDIA GPU for Hardware Encoding

If you have an NVIDIA GPU and want hardware encoding:
- Install latest NVIDIA drivers from https://www.nvidia.com/drivers
- NVENC is included in GeForce/Quadro drivers

## Building the Project

### Clone and Build

```powershell
# Navigate to project directory
cd broadcaster

# Install frontend dependencies
cd tauri-app/ui
npm install
cd ../..

# Build everything (debug mode - faster compile, slower runtime)
cargo build

# Build release version (slower compile, optimized runtime)
cargo build --release
```

### Common Build Commands

```powershell
# Check code compiles without building
cargo check

# Build and show warnings
cargo build 2>&1 | more

# Clean build artifacts (if something seems wrong)
cargo clean
```

## Running the Application

### Development Mode (with hot reload for UI)

```powershell
cd tauri-app
cargo tauri dev
```

This will:
1. Start the Vite dev server for the UI
2. Compile the Rust code
3. Launch the application
4. Auto-reload when you change UI files

### Production Mode

```powershell
cd tauri-app
cargo tauri build
```

The built application will be at:
- `tauri-app/target/release/broadcaster.exe`
- Installer: `tauri-app/target/release/bundle/msi/`

## Debugging

### Enabling Verbose Logging

The application uses the `tracing` crate with the `RUST_LOG` environment variable to control log verbosity.

**Log levels (from least to most verbose):** `error`, `warn`, `info`, `debug`, `trace`

```powershell
# Enable debug logging for all crates
$env:RUST_LOG = "debug"

# Enable trace logging (very verbose)
$env:RUST_LOG = "trace"

# Target specific crates
$env:RUST_LOG = "broadcaster_engine=debug"
$env:RUST_LOG = "broadcaster_engine=debug,broadcaster_capture=trace"

# Run with logging enabled
cargo tauri dev
```

### Viewing Logs

Logs appear in the terminal where `cargo tauri dev` was run. The codebase uses these logging macros:

- `error!` - Critical failures
- `warn!` - Potential problems
- `info!` - General status updates
- `debug!` - Detailed debugging info
- `trace!` - Very verbose, step-by-step execution

### Enabling Backtraces

When a panic occurs, enable backtraces to see the full call stack:

```powershell
# Basic backtrace
$env:RUST_BACKTRACE = "1"

# Full backtrace (includes all frames)
$env:RUST_BACKTRACE = "full"

# Example: run with both logging and backtrace
$env:RUST_LOG = "debug"
$env:RUST_BACKTRACE = "1"
cargo tauri dev
```

### Quick Build Scripts

The project includes PowerShell helper scripts in the root directory:

| Script | Purpose |
|--------|---------|
| `run-check.ps1` | Fast compilation check without building |
| `run-verbose.ps1` | Build with backtrace enabled |
| `run-clippy.ps1` | Run linting and code quality checks |

### Hot-Reloading Behavior

- **UI changes** (HTML/CSS/JS in `tauri-app/ui/`) hot-reload automatically
- **Rust changes** trigger automatic recompilation (slower, watch the terminal for progress)

### Adding Debug Output

To add temporary debug logging to investigate an issue:

```rust
use tracing::debug;

// Log a simple message
debug!("Reached this point in the code");

// Log variables (use {:?} for Debug trait)
debug!("my_variable = {:?}", my_variable);

// Log with context
debug!(frame_count = %count, "Processing frame");
```

## Code Quality Commands

Run these before committing changes:

```powershell
# Format code (auto-fix style issues)
cargo fmt

# Check for common mistakes and improvements
cargo clippy

# Run with warnings as errors (like CI does)
cargo clippy -- -D warnings

# Run tests
cargo test
```

## Project Structure Quick Reference

```
broadcaster/
├── Cargo.toml              # Workspace config (start here)
├── crates/
│   ├── broadcaster-engine/ # Core streaming logic
│   ├── broadcaster-capture/# Screen capture (WGC)
│   ├── broadcaster-audio/  # Audio capture (WASAPI)
│   ├── broadcaster-encoder/# Video/audio encoding
│   ├── broadcaster-transport/# RTMP streaming
│   └── broadcaster-ipc/    # UI<->Engine messages
└── tauri-app/
    ├── src/                # Tauri Rust code
    └── ui/                 # Web UI (HTML/CSS/JS)
```

## Architecture Overview

### Threading Model

```
┌─────────────┐
│  Tauri UI   │ (Main thread)
└──────┬──────┘
       │ IPC channels
┌──────▼──────┐
│ Orchestrator│ (Engine thread)
└──────┬──────┘
       │
   ┌───┴───┬───────────┬─────────────┐
   │       │           │             │
┌──▼──┐ ┌──▼──┐ ┌──────▼──────┐ ┌────▼────┐
│ WGC │ │WASAPI│ │   Encoder   │ │ Network │
│Capt.│ │(x2)  │ │(NVENC/x264) │ │ (tokio) │
└─────┘ └─────┘ └─────────────┘ └─────────┘
```

### Channel Capacities

| Channel | Capacity | Backpressure |
|---------|----------|--------------|
| Video capture → Encoder | 3 | Drop newest |
| Audio capture → Mixer | 8 | Block 5ms, then drop |
| Encoded → Network | 30 | Drop by priority |

## Troubleshooting

### "cargo not found"
Restart your terminal after installing Rust, or run:
```powershell
$env:Path = [System.Environment]::GetEnvironmentVariable("Path","User")
```

### Build fails with "link.exe not found"
Install Visual Studio Build Tools (see Prerequisites step 3)

### Build fails with "windows crate" errors
Make sure you're on Windows 10 version 1903 or later:
```powershell
winver  # Check your Windows version
```

### NVENC not working
- Check you have an NVIDIA GPU (RTX/GTX 600 series or newer)
- Update NVIDIA drivers
- The app will automatically fall back to x264 (CPU) encoding

### UI not loading
```powershell
cd tauri-app/ui
npm install  # Reinstall dependencies
npm run build  # Rebuild UI
```

## Useful Resources

- [Rust Book](https://doc.rust-lang.org/book/) - Learn Rust basics
- [Tauri Docs](https://tauri.app/v2/guides/) - Tauri framework guide
- [Cargo Book](https://doc.rust-lang.org/cargo/) - Package manager guide

## Quick Rust Tips for Newbies

```rust
// Rust uses Result<T, E> for error handling
let result = some_function()?;  // ? propagates errors

// No null! Use Option<T> instead
let maybe_value: Option<i32> = Some(42);
let nothing: Option<i32> = None;

// Variables are immutable by default
let x = 5;        // Can't change
let mut y = 5;    // Can change with 'mut'

// & means borrow (read-only reference)
// &mut means mutable borrow
fn example(data: &str) { }  // Borrows string, doesn't own it
```

## Known Limitations (v0.1)

1. No GPU texture zero-copy path (CPU copy for now)
2. Single audio device per type (no multi-mic)
3. Fixed 1080p output (no resolution options)
4. No bitrate adaptation
5. No scene switching
6. No preview window

## License

MIT
