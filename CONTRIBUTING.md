# Contributing to kiviTuner

Contributions are welcome across the Rust calibration/protocol workspace (`crates/*`), the Tauri backend (`src-tauri/`), the React calibration UI (`src/`), and documentation (`docs/`).

## Prerequisites

1. Install **Node.js 24+** (or Node 22.18+ for native TypeScript test execution via `node --test`).
2. Install the **Rust stable** toolchain (`rustup`, `cargo`, `rustc`).
3. On Linux, install the GTK/WebKit system packages listed in [docs/guides/building-and-packaging.md](docs/guides/building-and-packaging.md) when building the Tauri binary locally.

Read [README.md](README.md) for the workspace crate map and [DESIGN.md](DESIGN.md) for UI color tokens, AA contrast rules, and single-owner state guarantees.

## Development Workflow

```bash
npm ci
npm run dev
```

Regular commits and pull requests do not trigger the 5-OS installer build workflow; see [docs/guides/releasing.md](docs/guides/releasing.md) for how tagged releases work.

## Required Verification Before Opening a Pull Request

Run every check below from the repository root and confirm zero warnings and zero failures:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p kivi-core -p kivi-checksum -p kivi-formats -p kivi-flashing -p kivi-protocol -p kivi-j2534-bridge -p kivituner
npx tsc --noEmit
npm test
npm run build
```

## Engineering Rules

| Rule | Why It Matters |
|---|---|
| Preserve stock baseline isolation | Keep backend `original_memory` and frontend `originalRomData` immutable on load so delta views, change ledgers, and reverts remain exact. |
| Never fabricate hardware or test results | Mock/loopback tests prove codec and state-machine logic only. Document real physical hardware acceptance separately without invented VID/PID or vehicle support claims. |
| Keep live-hardware Write Lock default | Any live transport flash operation must pass size, stock backup, sector overlap, and explicit write-enable checks before transmitting write frames. |
| Write cross-platform tests and scripts | Never hardcode machine-local home paths in tests or docs; wrap globs in `package.json` with escaped double quotes (`\"src/**/*.test.ts\"`) so Windows `cmd.exe`, Linux, and macOS behave identically. |
| Keep UI and docs vendor-neutral | Avoid third-party commercial tuning software names in UI labels, tutorials, code comments, and public documentation. |

## License

By contributing to this repository, you agree that your contributions are licensed under the [MIT License](LICENSE).
