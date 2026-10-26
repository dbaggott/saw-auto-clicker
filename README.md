# saw-auto-clicker

My kiddo complained to me that the auto-clickers he was finding were from "shady ass websites" that he didn't trust.  He asked me to make one for him and so it was that the SAW auto-clicker was born.  SAW stands for Shady Azz Website and the full experience is at https://shadyazzwebsite.dnbg.app 

It works according to his specifications -- it can be enabled or disabled with a configurable hot key.  When enabled, holding the mouse button for longer than the configurable delay causes the mouse to start auto-clicking at the target clicks per second (also configurable).  It probably works for trackpad-based clicking but that hasn't been tested on anything other than a Windows running in a VM on MacOS.

The auto-clicker runs on Windows-only currently.  Open an issue if you want it on another OS.

## Install

Download and run the installer from the [latest release](https://github.com/dbaggott/saw-auto-clicker/releases/latest).

It installs for your user only. The installer isn't signed so you may get warnings about that from Microsoft Defender, if that happens, you'll need to navigate through allowing the application.

## How to use it

When you start saw-auto-clicker, its settings window opens. Closing the window leaves it running in the system tray (the notification area, near the clock), as a saw-blade icon. Click that icon to enable or disable auto-clicking, open the settings again, or quit. Windows may tuck a new icon behind the ^ arrow next to the clock; you can drag it out onto the taskbar to keep it in view. Starting saw-auto-clicker again while it's running opens its settings window.

Changes in the settings window are saved as you make them. The butterfly button sets the clicks per second to what a person can reach by butterfly clicking and turns on Jitter, which adds natural variation to the click rate.

The settings should all be self-explanatory. If they are not, open an issue requesting clarification.

## Additional Notes

### Administrator windows

saw-auto-clicker can't click into a window running as administrator, such as Task Manager or a program you started with **Run as administrator**. Windows blocks a normal program from sending input to an elevated one. The app doesn't offer to run as administrator itself, because that seems unnecessary and shady to me. Open an issue if you want that for some reason.

## Development

The workspace builds on any host. Windows-only code sits behind `cfg(windows)`, so on macOS or Linux `cargo clippy --workspace --all-targets` checks everything and `cargo test -p saw-core` runs the core's tests.

Releases are cut by bumping `version` in the workspace `Cargo.toml` and adding a `## v<version>` section to `CHANGELOG.md`, which becomes the release notes, then running `cargo check` so `Cargo.lock` records the new version too; CI builds with `--locked` and fails on a stale lock file. When a push to main carries a version that has no release yet, CI tests and builds it as usual, installs and uninstalls the installer on x64 and Arm64, checks that the installed app reports the new version, and publishes a GitHub release tagged `v<version>` at that commit with the installer attached (`.github/workflows/release.yml`). A version with a hyphen, such as `1.0.0-rc.1`, publishes a pre-release. The download page links to the latest release, which GitHub never resolves to a pre-release.

The installer is an [Inno Setup](https://jrsoftware.org/isinfo.php) script, `installer/saw-auto-clicker.iss`, which CI compiles once from the x64 and arm64 builds; it installs the one native to the machine (`.github/workflows/build.yml`). `.github/scripts/check-installer.ps1` is the install and uninstall check CI runs on it.

The app's icon file, `crates/app/assets/saw-auto-clicker.ico`, is drawn by `crates/app/src/icon.rs`, and a test fails when the two differ. After changing the drawing, regenerate the file with `cargo test -p saw-app write_the_icon_file -- --ignored`.

### saw-measure

`saw-measure` is a developer tool for measuring click timing, not part of the app, and releases don't include it. Build it with `cargo build --release -p saw-measure`, or download it from a CI run's build artifacts. Quit saw-auto-clicker before running it: both mark their clicks the same way, so with both running, each clicks and `saw-measure` counts the app's clicks as its own.

### The download page

`site/` is the project's download page, [shadyazzwebsite.dnbg.app](https://shadyazzwebsite.dnbg.app): a deliberately shady-looking parody of a freeware download site, built with Eleventy. `cd site && npm install && npm run dev` serves it locally; `npm run build && npm test` checks the build against the host's content security policy and the page's promise that nothing entered on it leaves the browser. CI publishes each build of main; deploying it to the live site is a separate step.

## License

MIT; see [LICENSE](LICENSE).
