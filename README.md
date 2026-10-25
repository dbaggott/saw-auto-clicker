# saw-auto-clicker

Hold-to-autoclick for the mouse. Hold the left or right button a moment longer than a normal click and it starts clicking that button for you, at a rate you set; let go and it stops. Short clicks, double-clicks and quick drags behave exactly as they always do.

It runs on Windows. macOS and Linux may follow later.

## How to use it

When it starts, saw-auto-clicker puts an icon in the notification area (the system tray, near the clock). It starts the way you left it, armed or disarmed, including when it starts at login. The first time you run it, it's **armed**.

- **Armed:** press a button and hold it past the activation delay (250 ms unless you change it). The app then clicks that button at the set rate until you release it. Left and right work independently, and holding both auto-clicks both. Only a press that begins while armed counts: a button you're already holding when you arm doesn't auto-click until you release it and press again.
- **Disarmed:** every click and hold reaches your applications unchanged.
- **The hotkey**, Ctrl+Alt+F8 unless you change it, switches between armed and disarmed from any application. Disarming during a burst stops it at once.
- **The tray icon** shows which state you're in, and hovering over it says so. Right-click it for the menu:
  - **Arm** / **Disarm**
  - **Settings…** opens the settings window.
  - **Start at login** starts the app when you sign in to Windows.
  - **Quit**

### Disarm for drags and text selection

While armed, *any* hold longer than the activation delay auto-clicks, and that includes dragging a window, selecting text, drawing, and long-presses. That's how hold-to-click works, not a fault. Press the hotkey to disarm before that kind of work, and again to arm when you're done.

## Download and install

Download one installer from the [latest release](https://github.com/dbaggott/saw-auto-clicker/releases/latest):

- `saw-auto-clicker-<version>-x64-setup.exe` for most Windows PCs (Intel or AMD processors).
- `saw-auto-clicker-<version>-arm64-setup.exe` for Windows on ARM, such as a Snapdragon laptop or a Windows virtual machine on an Apple Silicon Mac.

If you're not sure which you have, open **Settings > System > About** and read **System type**. An installer for the wrong one says so and stops.

Run it. It installs for your user only, so it doesn't ask for administrator rights, and it can start saw-auto-clicker at login if you tick **Start at login**. You get a Start menu entry, and you can uninstall from **Settings > Apps**. Installing a newer version over an older one upgrades it in place. If saw-auto-clicker is running, the installer and the uninstaller ask you to quit it first. Uninstalling keeps your settings file, so reinstalling picks them up again.

### Without installing

Each release also carries the app on its own, `saw-auto-clicker-<version>-x64.exe` and `saw-auto-clicker-<version>-arm64.exe`. Save the file somewhere it can stay, such as a `saw-auto-clicker` folder in your user folder, and double-click it to run. Running it again while it's already running just tells you so.

**Start at login** remembers where the file is. If you move the file later, the menu item shows unchecked; click it again to point it at the new location.

### "Windows protected your PC"

The release files aren't code-signed, so the first time you run one, installer or app, Microsoft Defender SmartScreen may show **Windows protected your PC**. Click **More info**, then **Run anyway**.

Microsoft Defender Antivirus may also flag or quarantine the file, because it watches mouse input and sends clicks, which some unwanted software also does. This hasn't been observed, but it's possible for an unsigned program like this one. If it happens, open **Windows Security > Virus & threat protection > Protection history**, select the entry for saw-auto-clicker, and choose **Allow on device**. Only do that for a file you downloaded from this project's releases page.

## Where it doesn't work

### Administrator windows

saw-auto-clicker can't click into a window running as administrator, such as Task Manager or a program you started with **Run as administrator**. Windows blocks a normal program from sending input to an elevated one. When you hold a button over such a window, the app doesn't auto-click: Windows doesn't let the app see that press at all, so the hold acts as a single normal press. If you're already auto-clicking and move the pointer onto such a window, the app is built to stop clicking and show a tooltip on the tray icon saying clicking is unavailable over elevated windows; that case hasn't been tested yet. The app doesn't offer to run as administrator itself, because a mouse hook running with administrator rights could do far more damage if it misbehaved.

### Touchpads

On a Windows on ARM virtual machine on an Apple Silicon Mac (VMware Fusion), the Mac's trackpad works like a mouse: pressing the trackpad down and holding past the delay auto-clicks, and releasing stops cleanly. A tap is too short to reach the delay, so it stays a normal click. A two-finger press is a right click and auto-clicks the same way.

On a real Windows laptop's precision touchpad, behaviour is **unverified**: how Windows reports touchpad clicks and taps to the app isn't documented, and it hasn't been tested on such hardware. A physical mouse is the supported way to use it.

## Settings

Open **Settings…** from the tray menu and click **Apply** to save your changes. A new hotkey works at once; the other settings take effect from your next press.

| Setting | Range | Default | What it does |
| --- | --- | --- | --- |
| Clicks per second | 1–50 | 10 | How fast it clicks during a hold. |
| Activation delay | 50–2000 ms | 250 ms | How long you hold before auto-clicking starts. The shorter it is, the more often an ordinary slow click turns into auto-clicking. |
| Jitter | 0–50% | 0% | Lets the gap between one click and the next vary randomly by up to this share of the normal gap, so the clicks aren't perfectly even. The average rate stays the same. |
| Toggle hotkey | see below | Ctrl+Alt+F8 | Switches between armed and disarmed. |

The hotkey is one of F1–F24, A–Z or 0–9, with any of Ctrl, Alt, Shift and Win. A letter or digit needs Ctrl, Alt or Win, since on its own (or with only Shift) it would stop that key typing in every application; the settings window refuses one without, and nothing is applied until you fix it. F12 is reserved by Windows. If another application already uses the combination you pick, the settings window says so and your previous hotkey stays in effect.

The settings are saved to `%APPDATA%\saw-auto-clicker\config.toml`, for example `C:\Users\<you>\AppData\Roaming\saw-auto-clicker\config.toml`. You don't need to edit it, but if you do, quit the app first:

```toml
cps = 10
activation_delay_ms = 250
jitter_percent = 0
hotkey = "Ctrl+Alt+F8"
armed = true
```

`armed` is the state the app starts in, and the app rewrites the file each time you arm or disarm. A missing file or a missing value means the default. A value that's invalid falls back to its default, and the settings window shows a warning naming it. If the saved hotkey can't be registered when the app starts, the tray tooltip says no hotkey is active; open Settings to pick another.

## saw-measure

Each release also carries `saw-measure-<version>-x64.exe` and `saw-measure-<version>-arm64.exe`. It's a developer tool for measuring click timing, not part of the app, and you don't need it. Quit saw-auto-clicker before running it: both mark their clicks the same way, so with both running, each clicks and `saw-measure` counts the app's clicks as its own.

## Development

The workspace builds on any host. Windows-only code sits behind `cfg(windows)`, so on macOS or Linux `cargo clippy --workspace --all-targets` checks everything and `cargo test -p saw-core` runs the core's tests.

Releases are cut by bumping `version` in the workspace `Cargo.toml`, then running `cargo check` so `Cargo.lock` records the new version too; CI builds with `--locked` and fails on a stale lock file. When a push to main carries a version that has no release yet, CI tests and builds it as usual, installs and uninstalls the x64 installer, checks that the x64 binaries launch, and publishes a GitHub release tagged `v<version>` at that commit with the six files attached: the app, its installer and `saw-measure`, each for x64 and arm64 (`.github/workflows/release.yml`). A version with a hyphen, such as `1.0.0-rc.1`, publishes a pre-release. The download page links to the latest release, which GitHub never resolves to a pre-release.

The installer is an [Inno Setup](https://jrsoftware.org/isinfo.php) script, `installer/saw-auto-clicker.iss`, which CI compiles for each architecture (`.github/workflows/build.yml`). `.github/scripts/check-installer.ps1` is the install and uninstall check CI runs on it.

The app's icon file, `crates/app/assets/saw-auto-clicker.ico`, is drawn by `crates/app/src/icon.rs`, and a test fails when the two differ. After changing the drawing, regenerate the file with `cargo test -p saw-app write_the_icon_file -- --ignored`.

### The download page

`site/` is the project's download page, [shadyazzwebsite.dnbg.app](https://shadyazzwebsite.dnbg.app): a deliberately shady-looking parody of a freeware download site, built with Eleventy. `cd site && npm install && npm run dev` serves it locally; `npm run build && npm test` checks the build against the host's content security policy and the page's promise that nothing entered on it leaves the browser. CI publishes each build of main; deploying it to the live site is a separate step.

## License

MIT; see [LICENSE](LICENSE).
