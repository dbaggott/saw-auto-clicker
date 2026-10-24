# saw-auto-clicker
Hold-to-autoclick for the mouse: hold a button past a short delay and it clicks at a set rate with optional jitter. Windows first, macOS and Linux later.

## Development

The workspace builds on any host. Windows-only code sits behind `cfg(windows)`, so on macOS or Linux `cargo clippy --workspace --all-targets` checks everything and `cargo test -p saw-core` runs the core's tests.

## License

MIT; see [LICENSE](LICENSE).
