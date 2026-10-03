# Changelog

## 0.3.2 — 2026-10-28

- Fix taskbar clocks briefly disappearing behind the panel when clicking app buttons: the clock popup is now owned by the taskbar, and stacking changes are handled immediately instead of waiting for the refresh timer.
- Recreate the clock popup if Explorer destroys and rebuilds its taskbar.
- Keep the same stacking protection on horizontal and vertical taskbars without activating the clocks or intercepting clicks.
- Add a native Windows regression test for taskbar stacking and a read-only `--watch-taskbar` diagnostic.

## 0.3.1 — 2026-10-28

- Support left and right taskbars with vertically stacked clocks and time text sized to fit narrow panels.
- Correct fullscreen detection so ordinary maximized windows and Explorer's switcher are not mistaken for fullscreen applications.
- Retain the last valid layout through brief accessibility failures and update layered-window pixels, position, and size together to avoid blank frames.
- Respect auto-hide on every screen edge and avoid repainting unchanged taskbar snapshots.

## 0.3.0 — 2026-10-28

Initial public release.

- Portable Windows executable with an English interface and per-user registry settings.
- Click-through desktop cards without window frames or taskbar buttons.
- Separate clock selections for the desktop and taskbar, with up to 12 clocks.
- Transparent taskbar time row that works independently of the native Windows clock setting.
- Visual card resizing and draggable text elements in the appearance editor.
- Installed Windows font discovery, content templates, and per-clock styles.
- Independent colors and opacity for text, text outlines, card backgrounds, and borders.
- Windows time-zone conversion, daylight saving support, optional startup, and 12/24-hour display.
