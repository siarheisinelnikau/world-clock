# Contributing

World Clock is a native Windows application written in Rust. Bug reports and focused pull requests are welcome.

For a bug report, include your Windows version, display scaling, taskbar configuration, steps to reproduce, and what you expected. Screenshots help with layout problems. Remove personal information from screenshots and diagnostic logs before sharing them.

## Development

Use 64-bit Windows, stable Rust with the `x86_64-pc-windows-msvc` toolchain, and Visual Studio C++ Build Tools with the Windows SDK.

```powershell
cargo run -- --settings
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
.\build.ps1
```

Check desktop click-through behavior, positioning, settings persistence, and taskbar placement manually when changing those features. Include the relevant checks in your pull request.

Quit a running copy from the tray before rebuilding into the same `dist/WorldClock.exe` path; Windows locks executables while they are running.

## Source map

| File | Responsibility |
| --- | --- |
| `src/app.rs` | Application windows, settings, tray, and event handling |
| `src/appearance.rs` | Card styles and the visual appearance editor |
| `src/ui.rs` | Rendering, text layout, and layered windows |
| `src/taskbar.rs` | Taskbar discovery and placement |
| `src/time.rs` | Windows time zones and formatting |
| `src/settings.rs` | Registry persistence and settings migration |

`--preview-style` opens an isolated appearance editor without changing saved settings. `--verify-overlays` writes `overlay-verification.log` with Windows hit-test and position checks. `--inspect-taskbar` writes taskbar accessibility diagnostics. Logs and build output are ignored by Git.

## Releases

GitHub Actions checks formatting, runs Clippy and tests, and builds the executable on Windows. Pull requests produce a downloadable build artifact.

After a successful push to `main`, the workflow publishes a release **only when the version in `Cargo.toml` has no existing GitHub release**. For a new release, update the package version and `CHANGELOG.md` together. Existing releases are left unchanged. A manual workflow run on `main` can retry publication after a failed run.

Release assets are `WorldClock.exe` and `SHA256SUMS.txt`. Build products belong in releases, not in the source tree. Contributions are provided under the project's [MIT license](LICENSE).
