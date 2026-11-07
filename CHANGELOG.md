# Changelog

## 0.5.3 — 2026-11-07

Reveal desktop clocks directly from the notification-area icon while keeping the context menu independent.

### Fixed

- A single left click shows existing desktop clocks above ordinary application windows instead of opening Settings or leaving the clocks covered. Hidden and minimized clocks are restored, repeated clicks keep them visible, and no window opens when no clocks have **On desktop** enabled.
- Right-click opens the existing native menu without showing or raising the desktop clocks. A separate hidden tray host owns the menu so its activation cannot bring the clock panel forward.
- Keep the saved **Always on top** preference and click-through styles unchanged when revealing clocks. Settings, Updates, and Quit remain available from the right-click menu.

### Verification and scope

- The isolated desktop diagnostic runs 18 checks through the production tray callback, including actual native window stacking, hidden/covered/minimized restoration, repeated activation, the empty desktop selection, and the real native context menu.
- Settled visible, hidden, minimized, and restored samples produce no repeated rasterization or presentation. Physical mouse routing through Explorer's tray, mixed-monitor interaction, GPU timing, and input latency remain outside these checks.
- Portable Windows x64 executable: `WorldClock-v0.5.3.exe`. Existing clocks, styles, language, theme, startup checks, and other saved settings are preserved.

## 0.5.2 — 2026-11-03

Reserve real taskbar space for clocks without installing Windhawk.

### Added

- **Reserve space for clocks** reduces the application-button area on a standard horizontal Windows 11 taskbar so buttons stop before the clocks and enter overflow sooner. Width follows the selected clocks and display scaling while retaining room for applications and preserving the full-width taskbar background.
- Restore the original taskbar layout when changing modes, removing all taskbar clocks, or quitting. A process-exit watchdog also restores the layout after an unexpected exit.
- Run reservation setup in the background and show its state or a recoverable failure in Settings. A small native module is embedded in the portable executable and extracted beside it only when this mode is used; no external installation or Explorer restart is needed.

### Improved

- Replace the hiding checkbox with three placement choices: **Hide when space is limited**, **Show over app buttons**, and **Reserve space for clocks**. Existing true/false settings migrate to the matching first or second choice. The separate backdrop preference is preserved.
- Translate the placement choices and reservation status into all six interface languages, and ignore offscreen taskbar buttons when measuring visible space.

### Verification and scope

- Formatting, Clippy, and 66 tests pass; one existing network test remains intentionally ignored. Coverage includes preference migration, reservation geometry at 100–200% scaling, and stale background-operation results.
- A real-taskbar diagnostic verifies reservation, application-button displacement, restoration, and settled CPU/layout behavior. A separate disposable-owner test verifies restoration after forced termination. Native Settings captures cover the new selector; isolated GDI diagnostics cover visible, hidden, minimized, and restored clock windows.
- Reservation targets the standard horizontal Windows 11 taskbar. Windows 10, vertical taskbars, and modified shells retain Hide and Show over app buttons. Interactive overflow clicking, mixed-monitor reservation, GPU timing, and input latency are not covered by these checks.
- Portable Windows x64 executable: `WorldClock-v0.5.2.exe`. Existing clocks, styles, language, theme, and update preferences are retained.

## 0.5.1 — 2026-11-03

Choose whether taskbar clocks hide when application buttons take their space, and independently control the text backdrop.

### Added

- **Hide when taskbar is crowded** in Settings. Disable it to keep clocks visible over app buttons while preserving mouse pass-through and leaving the native system tray clear. Fullscreen and taskbar auto-hide behavior is preserved.
- **Shade background behind text**, independent of hiding. Use a dark backdrop behind light text or a light backdrop behind dark text, with soft edges and a short, finite fade when its state changes. Disable it for transparent text alone.
- Both options are translated into all six interface languages and saved with the existing clock settings. Existing settings default to both options enabled.

### Fixed

- Pin CI to the same shared updater revision used for local verification, including its existing fix for update preparations created within the same Windows clock tick.

### Verification and scope

- Formatting, Clippy, and 64 tests pass; one existing network test is intentionally ignored. Tests cover crowded layouts on all four taskbar edges at 100%, 125%, 150%, and 200% scaling, preference migration, backdrop alpha, and transition completion.
- Native Settings captures and app-generated taskbar-renderer proofs cover the new controls and light/dark backdrops. An isolated optimized taskbar diagnostic exercises both fade directions and visible, hidden, minimized, and restored states without attaching to Explorer. Actual Explorer overlap interaction, mixed-monitor interaction, GPU timing, and input latency are outside this diagnostic's scope.
- Portable Windows x64 executable: `WorldClock-v0.5.1.exe`. Existing clocks, layouts, interface preferences, and update settings are retained.

## 0.5.0 — 2026-11-01

Choose your preferred language and interface theme without restarting World Clock.

### Added

- Complete English, Russian, French, German, Spanish, and Italian interface catalogs covering Settings, the appearance editor, tray commands, update actions, dates, and relative-day labels.
- Discoverable Language and Light/Dark controls in Settings, with immediate application and persistent choices. Existing settings migrate to English and Light without changing clock names, layouts, templates, or update preferences.
- An isolated `--capture-settings` mode that prints the real native Settings client to PNG and exits without loading user settings, starting monitoring, creating a tray icon, or contacting GitHub.

### Improved

- Widen Settings and its primary controls to accommodate translated labels.
- Use a native themed Updates window with visible installed version, startup-check opt-out, manual checking, and explicit download/install actions.
- Apply the same native interface palette to Settings, Appearance, and Updates while preserving each clock's custom face colors.

### Verification and scope

- Formatting and Clippy pass; the test suite passes 59 tests, with one existing network test intentionally ignored. Catalog/placeholder coverage and preference migration are included.
- Native Settings captures cover all six languages and both themes. An isolated optimized clock diagnostic records zero paints, rasterizations, and presents during settled visible, hidden, minimized, and restored samples. Tray interaction, Settings keyboard/accessibility traversal, common Windows dialogs, mixed-monitor interaction, GPU timing, and full background-service performance require separate validation.
- Windows x64 standalone executable: `WorldClock-v0.5.0.exe`; portable settings remain in the current user's registry.

## 0.4.5 — 2026-11-01

Make the notification-area clock icon larger and easier to recognize.

### Improved

- Enlarge the clock face and thicken the outline and hands in the 16, 20, 24, and 32 pixel icons, improving readability on light and dark taskbars.
- Add a native 20 pixel icon for 125% display scaling alongside the existing sizes for 100%, 150%, and 200% scaling.
- Load the tray icon at Windows' small-icon size instead of shrinking the large application icon, and release the temporary icon handle after the shell copies it.

## 0.4.4 — 2026-10-31

Keep the Appearance preview stable while mechanical clock hands animate.

### Fixed

- Compose the checkerboard, clock face, hands, resize handles, and text selection offscreen and present the complete preview in one operation, preventing visible intermediate frames.
- Handle preview painting and background erasure directly so the Windows static control cannot clear the animated clock between frames. Preserve live animation, resizing, and text positioning.
- Add regression coverage that keeps the previous frame intact during rendering for mechanical and digital previews at 100%, 125%, 150%, and 200% scaling, including nonzero drawing origins and empty frames.

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
