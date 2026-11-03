# Changelog

Each release's notes are its section here. CI fails when the workspace version in `Cargo.toml` has no section, so bumping the version means writing one.

## v2.0.0

- Auto-clicking now happens only in the applications you allow, so holding a button to drag files, select text or move a window in any other program works as usual. Allow applications in Settings, under Applications, from the programs that are running.
- After upgrading, no applications are allowed, so the app auto-clicks nowhere until you add one. The settings window and the tray icon's tooltip say so while none are allowed.
- The settings window has an Enabled switch, which shows and changes the same state as the tray menu and the hotkey.
- A new install now has Enable on startup on. An upgraded install keeps its setting.
- When the app starts enabled, a Windows notification says so, and how many applications it auto-clicks in.
- The settings window's taskbar icon now shows whether the app is enabled, as the tray icon does.
- Windows forgets once whether you had the tray icon shown on the taskbar or in the overflow area; set it again if you had moved it.

## v1.0.0

- The butterfly button now sets 15 clicks per second, up from 12. A saved 12 with Jitter on no longer shows as butterfly clicking.
- Jitter now also varies the click rate during a hold, drifting up to 2 clicks per second either side of the set rate at 15, proportionally at other rates, and never above 50, as a person's clicking speeds up and slows down.

## v0.6.1

- Reworded the explanations for the Clicks and Enable on startup settings and the butterfly button.

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
