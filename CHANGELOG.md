# Changelog

Each release's notes are its section here. CI fails when the workspace version in `Cargo.toml` has no section, so bumping the version means writing one.

## v0.6.0

- Closing the settings window shows the saw-blade icon flying into the system tray, so you can see where the app went. It is skipped when Animation effects are off in Windows' settings.

## v0.5.0

- The installer's last page explains that the app lives in the system tray. Starting the app opens its settings window, and starting it again while it runs opens that window instead of a message. Start at login still starts it quietly in the tray.
- Settings are saved as you change them; there is no Apply button.
- New setting, Clicks: which mouse buttons auto-click, left only, both, or right only.
- Jitter is now on or off. On adds natural variation to the click rate, roughly as much as a person's clicking has. A saved Jitter above 0% comes back as on.
- New butterfly button: sets the clicks per second to what a person can reach by butterfly clicking, with Jitter on.
- Clicking an info icon no longer hides its explanation.
- "Armed" and "disarmed" are now "enabled" and "disabled" everywhere, including the tray menu and the Enable on startup setting, which keeps its saved value.

## v0.4.0

- Each setting has an info icon; hover over it to see what the setting does.
- A refreshed settings window: settings grouped into panels, an on/off switch for Arm on startup, and a window that grows to fit any messages instead of hiding them.

## v0.3.0

- No hotkey is set by default; choose one in Settings.
- New setting: Arm on startup, off by default. The app no longer remembers whether it was armed when it closed.
- The first launch after a fresh install starts armed.
- One installer for every PC; it installs the right version for yours.
