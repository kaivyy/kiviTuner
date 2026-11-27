# Building & Packaging Desktop Installers

kiviTuner compiles into native desktop installers across Windows (64-bit & 32-bit), Linux (`x86_64`), and macOS (Apple Silicon & Intel).

---

## 1. CI Release Matrix

Defined in [.github/workflows/build-desktop.yml](../../.github/workflows/build-desktop.yml) and [tauri.conf.json](../../src-tauri/tauri.conf.json):

| CI Artifact Name | Runner & Rust Target | Bundle Outputs | Notes |
|---|---|---|---|
| `kivituner-windows-x64` | `windows-latest` / `x86_64-pc-windows-msvc` | NSIS `.exe`, WiX `.msi` | Recommended Windows build. Compiles `kivi-j2534-bridge` for `i686-pc-windows-msvc` and bundles `binaries/kivi-j2534-bridge-x86.exe` via [tauri.windows-x64.conf.json](../../src-tauri/tauri.windows-x64.conf.json) so 64-bit direct and 32-bit bridged J2534 DLLs both work. |
| `kivituner-windows-x86` | `windows-latest` / `i686-pc-windows-msvc` | NSIS `.exe`, WiX `.msi` | Native 32-bit build for 32-bit Windows systems or direct in-process 32-bit J2534 DLL loading (cannot load 64-bit DLLs). |
| `kivituner-linux` | `ubuntu-22.04` / `x86_64-unknown-linux-gnu` | `.deb`, `.rpm`, `.AppImage` | Uses `libwebkit2gtk-4.1`, `libudev`, and Linux inbox `SocketCAN`/`cdc_acm` transports. |
| `kivituner-macos-arm64` | `macos-latest` / `aarch64-apple-darwin` | `.dmg`, `.app.tar.gz` | Native Apple Silicon build. |
| `kivituner-macos-x64` | `macos-latest` / `x86_64-apple-darwin` | `.dmg`, `.app.tar.gz` | Intel x86_64 macOS build (`minimumSystemVersion: 10.15`). |

### Triggering a GitHub Actions Build
- **Tagged release build (draft GitHub Release + downloadable artifacts):**
  ```bash
  git tag v0.1.0
  git push origin v0.1.0
  ```
- **Manual on-demand build:** Open the **Actions** tab on GitHub -> **Build Desktop Installers** -> **Run workflow**.

---

## 2. Local Native Builds

### Prerequisites (All Platforms)
- **Node.js 24+** (or 22.18+ for native TypeScript test execution) and `npm`
- **Rust stable toolchain** (`cargo`, `rustc`, `rustup`)
- **Tauri CLI** (invoked via `npx --yes @tauri-apps/cli@2 build` or `cargo tauri build`)

Install frontend packages once from the repository root:
```bash
npm ci
```

---

### Linux (`x86_64-unknown-linux-gnu`)
Install system libraries (Debian / Ubuntu):
```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  patchelf \
  libfuse2 \
  rpm \
  libssl-dev \
  libgtk-3-dev \
  libudev-dev \
  pkg-config
```

Build `.deb`, `.rpm`, and `.AppImage` *(set `NO_STRIP=true` on Ubuntu 22.04+ if AppImage linuxdeploy strip hits newer distro library sections)*:
```bash
NO_STRIP=true npx --yes @tauri-apps/cli@2 build
```
Artifacts appear under `target/release/bundle/{deb,rpm,appimage}/`.

---

### Windows 64-bit with 32-bit J2534 Bridge (`x86_64-pc-windows-msvc`)
Requires Microsoft C++ Build Tools (MSVC x64 & x86 targets) and WebView2 runtime.

1. Add both Rust targets and compile the 32-bit J2534 bridge helper:
   ```powershell
   rustup target add x86_64-pc-windows-msvc i686-pc-windows-msvc
   cargo build --release --target i686-pc-windows-msvc -p kivi-j2534-bridge
   New-Item -ItemType Directory -Force -Path src-tauri/binaries | Out-Null
   Copy-Item target/i686-pc-windows-msvc/release/kivi-j2534-bridge.exe src-tauri/binaries/kivi-j2534-bridge-x86.exe -Force
   ```
   *(If you run only `cargo run` or `npm run tauri dev` locally without bundling, placing `kivi-j2534-bridge-x86.exe` beside the built executable or setting `KIVI_J2534_BRIDGE_32` enables bridged 32-bit DLL loading immediately.)*

2. Build the x64 installer with the Windows x64 resource overlay:
   ```powershell
   npx --yes @tauri-apps/cli@2 build --target x86_64-pc-windows-msvc --config src-tauri/tauri.windows-x64.conf.json
   ```
Artifacts appear under `target/x86_64-pc-windows-msvc/release/bundle/{nsis,msi}/`.

---

### Windows 32-bit Native (`i686-pc-windows-msvc`)
```powershell
rustup target add i686-pc-windows-msvc
npx --yes @tauri-apps/cli@2 build --target i686-pc-windows-msvc
```
Artifacts appear under `target/i686-pc-windows-msvc/release/bundle/{nsis,msi}/`.

---

### macOS (`aarch64-apple-darwin` or `x86_64-apple-darwin`)
Requires Xcode Command Line Tools (`xcode-select --install`).
```bash
# Apple Silicon:
rustup target add aarch64-apple-darwin
npx --yes @tauri-apps/cli@2 build --target aarch64-apple-darwin

# Intel macOS:
rustup target add x86_64-apple-darwin
npx --yes @tauri-apps/cli@2 build --target x86_64-apple-darwin
```

---

## 3. Local Verification Suite

```bash
# 1. Portable check without FFI dynamic loading
cargo check -p kivi-flashing -p kivi-protocol --no-default-features

# 2. Full workspace static analysis (warnings denied)
cargo clippy --workspace --all-targets -- -D warnings

# 3. Hardware transport, discovery, bridge IPC, protocol, and Tauri backend tests
cargo test -p kivi-flashing -p kivi-protocol -p kivi-j2534-bridge -p kivituner

# 4. Frontend typecheck, unit tests, and Vite production bundle
npx tsc --noEmit
npm test
npm run build
```
