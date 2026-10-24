# saw-auto-clicker
Hold-to-autoclick for the mouse: hold a button past a short delay and it clicks at a set rate with optional jitter. Windows first, macOS and Linux later.

## Development

The workspace builds on any host. Windows-only code sits behind `cfg(windows)`, so on macOS or Linux `cargo check --workspace` checks everything and `cargo test -p saw-core` runs the core's tests. `rust-toolchain.toml` pins the toolchain and the two Windows targets; CI tests on `windows-latest` and builds x64 and arm64 binaries of `saw-app` and `saw-measure` as workflow artifacts.

## License

MIT; see [LICENSE](LICENSE).
