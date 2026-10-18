# Changelog

## 0.4.3 — 2026-10-30

Improve taskbar text fitting and reduce CPU work when clocks have not changed.

### Fixed

- Fit taskbar labels and times using measured font heights so short panels do not cut off the text. Reduce text sizes when necessary while keeping the content centered.
- Use a consistent time font across taskbar clocks and rotating groups. One- and two-digit hours in 12-hour mode no longer produce different text sizes.
- Add rendering checks for short panels and verify every minute of the day fits the shared font in both time formats at 100%, 125%, 150%, and 200% scaling.

### Improved

- Cache unchanged digital card pixels, with invalidation for content, style, and DPI changes and a 16 MB pixel limit. Sweeping dials no longer re-rasterize neighboring digital cards on every frame.
- Enable the 50 ms desktop timer only while a visible clock has a sweeping second hand. Refresh other clocks once per displayed second or minute and avoid duplicate frames from overlapping timers.
- Retain completed desktop frames through paint events, skip hidden-window timer rendering, and repaint taskbar clocks only when the time, group, layout, or appearance changes.
- Match the appearance preview's timer to its clock type and update digital seconds without continuously repainting static previews.

## 0.4.2 — 2026-10-30

### Fixed

- Compact taskbar clocks to fit more selected time zones before rotating groups. Four horizontal clocks can now share a gap of about 252 logical pixels, including spacing, instead of rotating when all four could fit.
- Keep every available slot filled on the final group. With four selected clocks and three slots, alternate clocks 1–3 and 2–4 every six seconds instead of showing one clock with two empty slots.
- Verify compact layout boundaries and populated overflow groups on horizontal and vertical taskbars at 100%, 125%, 150%, and 200% scaling, in both time formats.

## 0.4.1 — 2026-10-30

### Added

- Check GitHub Releases for updates at startup, with an opt-out in Settings and manual checks in Settings and the tray menu. Show the running version and update status.
- Download and install a newer stable release from the app, with progress, SHA-256 verification, saved settings, and automatic restart. Preserve the portable executable's name and location and restore the previous executable if Windows cannot launch the update.
- Keep update checks and downloads in the background. Connection errors leave the current application running and can be retried.

### Improved

- Name release executables `WorldClock-vVERSION.exe` and match their exact versioned names when selecting downloads and verifying checksums. Keep each installed executable's existing path during updates so startup shortcuts remain valid.
- Simplify the README to one application example and document updating and release naming conventions.

### Upgrade

- Versions before 0.4.1 need one manual download to gain in-app updates. Releases target 64-bit Windows 10/11; no extra runtime is required. Published older releases keep their original filenames.

## 0.4.0 — 2026-10-30

- Keep desktop clocks above Explorer's desktop after Win+D, without activating them or enabling Always on top. Retain click-through input, positioning, and the user's Show/Hide selection.
- Add per-clock mechanical dials with Arabic/Roman/no numerals, minute marks, name/date, hand widths and colors, and an independent smooth or ticking second hand. Appearance previews animate live; older saved styles stay digital.
- Cache stationary dial faces for smooth hand animation without rasterizing the numerals and marks every frame.
- Update the user guide and README with mechanical clock setup and a screenshot showing mechanical and digital clocks together.

## 0.3.3 — 2026-10-29

- Search the Windows time-zone list by city, zone name, or UTC offset, with case-insensitive multi-word matching and a Clear button. Searching does not change a clock until a result is selected.
- Read registry settings using their actual size so large custom card layouts survive restarting the app.
- Show the correct relative day when clocks are two calendar days apart across the international date line, including year and leap-day boundaries.

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
