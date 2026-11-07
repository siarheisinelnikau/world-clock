# Using World Clock

## Choose your clocks

Open **Settings** from the clock icon in the notification area. Windows may initially put it in the overflow menu (`^`). Launching the downloaded executable again also opens Settings in the running instance.

Add up to 12 clocks, give them useful labels, and reorder them with the arrow buttons. Each clock has independent **On desktop** and **In taskbar** switches. **Local** follows the Windows time zone; it can be hidden from both surfaces but cannot be removed. Other clocks need at least one display destination.

Use the search box under **Time zone** to filter the list by a city shown in Windows' zone descriptions, a zone name, or an offset such as `UTC+05:30`. Search is case-insensitive and accepts several words, for example `London UTC`. Choose a result from the dropdown to change the clock's zone. Typing a search or getting no matches keeps the current zone unchanged. **Clear** restores the full list; selecting another clock also clears the search.

Optional settings include 12-hour time, desktop seconds, always on top, and starting with Windows. Startup is off by default.

## Interface language and theme

The **Language** and **Theme** dropdowns in Settings apply immediately and save independently of unfinished clock edits. Choose English, Русский, Français, Deutsch, Español, or Italiano; choose Light or Dark. Existing installations start in English with the Light interface. Language changes include Settings, Appearance, Updates, tray commands, date names, and relative days. Clock labels, content templates, and Windows time-zone identifiers remain unchanged. Windows supplies localized zone descriptions and native common dialogs; detailed technical failures may retain their originating language.

The interface theme covers Settings, Appearance, and Updates. A clock's own face colors and transparency are part of its saved design and remain independent of this preference.

## Position the desktop cards

Left-click the notification-area icon to bring clocks with **On desktop** enabled above ordinary application windows. This restores hidden or minimized clocks without opening Settings, changing **Always on top**, or hiding clocks on repeated clicks. If no desktop clocks are selected, nothing opens. Right-click opens only the usual menu, including **Settings** and **Quit World Clock**, without showing or raising the clock panel.

Choose **Save & position clocks…** in Settings, or **Position desktop clocks…** in the tray menu. Drag the cards into place, then double-click to lock them or choose **Lock desktop clocks** from the tray menu. Locking preserves the position and makes clicks and scrolling pass through again.

Desktop cards normally sit behind application windows. **Always on top** keeps them above applications while still passing mouse input through. Tall lists use additional columns.

**Win+D** (Show Desktop) leaves the clocks visible over the desktop. They remain click-through, keep their position, and stay below ordinary applications unless **Always on top** is enabled.

## Design a card

Select a clock and open **Appearance…**. Changes appear immediately in the preview. Choose **Use style**, then **Save** in Settings to keep them. **Apply this style to all clocks** copies the design to the other clocks.

### Mechanical clocks

Choose **Mechanical (hands)** under **Clock type**. Each dial uses its clock's time zone, including daylight saving time. Taskbar clocks keep their compact digital display.

The **Dial** tab controls Arabic or Roman numerals (or none), minute marks, the clock name and date, hand width, and separate hour, minute, and second hand colors. Enable **Second hand** and **Smooth sweep** for continuous movement; turn off Smooth sweep for one tick per second. The dial's second hand is independent of the desktop digital seconds setting.

Use **Card** to resize the clock and change padding and space between clocks. **Auto** gives the dial a square canvas; with an explicit rectangular size, the round dial fits inside it. **Colors** controls the circular face background, rim, numerals and marks, and their opacity. Text opacity also controls the hands. A 0% background leaves the hands, marks, and any enabled rim visible. **Content** controls the font, numeral weight, and name/date detail size. Digital templates and text positions are preserved when switching clock types.

### Card

Drag the blue handles on the preview edges or corners to resize. Width is 120–600 logical pixels; explicit height is 24–700. **Auto** fits height to the content. Padding, space between cards, and corner radius are configurable. Dimensions scale with Windows display scaling.

### Content

Choose a preset or edit up to eight lines. Templates support `{name}`, `{time}`, `{date}`, `{offset}`, and `{day}`, plus literal text. Lines containing `{time}` use the time font size; other lines use the detail size.

`{day}` compares the clock's calendar date with your local Windows date: **Today**, **Yesterday**, or **Tomorrow**. Clocks across the international date line can be two calendar days apart; these show **In 2 days** or **2 days ago**.

The font picker lists installed Windows font families, including fonts installed for your user account. Scroll the list or type a font name. Reopen Appearance after installing a font to refresh the list.

Drag the time, name, UTC offset, date, or day directly in the preview. A dotted outline marks the selected element, with its coordinates below the preview. **Reset text positions** returns to the template layout. Editing the template or alignment resets custom positions. Press **Esc** during a drag to cancel it.

### Colors

Text, text outline, card background, and card border each have their own color and opacity (0–100%). Text outline and card border also have separate widths (0–4 pixels). Setting background opacity to 0% leaves the other enabled layers visible. **Reset to compact** restores the default style in the editor.

## Taskbar behavior

The extra clocks sit in the free gap between app buttons and the primary taskbar's tray. They use transparent text and pass mouse input through. Top and bottom taskbars use a horizontal row; left and right taskbars use a vertical stack, with time text adjusted to fit the panel width. The native Windows clock stays visible according to your Windows settings. Hiding the native clock does not hide World Clock.

Time text uses a common font size across clocks and rotating groups, including one- and two-digit hours in 12-hour mode. Short panels reduce label and time sizes using measured font heights to keep the text centered and prevent clipping. Extremely short custom panels show only the time when both lines cannot fit.

Approximately 200 logical pixels of horizontal space or 140 logical pixels of vertical space are needed for three slots. Clocks compact to fit more selected zones before rotating; four horizontal clocks need about 252 logical pixels, including spacing. When selected zones still exceed the available slots, groups switch every six seconds. The final group overlaps earlier clocks in saved order so every slot stays filled: four clocks sharing three slots alternate clocks 1–3 and 2–4. With the default hiding option enabled, clocks hide if the taskbar is too full for the minimum group. Fullscreen and taskbar auto-hide are respected on every edge. Ordinary maximized windows and Explorer's task switcher keep the clocks visible. Brief accessibility failures retain the last valid layout instead of clearing the clocks. Placement targets the primary Windows taskbar; modified shells may expose a different accessibility structure.

**Placement** has three choices:

- **Hide when space is limited** hides the clocks if app buttons leave too little room.
- **Show over app buttons** keeps clocks visible over application buttons.
- **Reserve space for clocks** leaves a real gap before the tray by reducing the available application-button area. Buttons use overflow sooner. This mode is built in and requires the standard horizontal Windows 11 taskbar; Windhawk is not required. Other taskbars can continue using the first two modes.

Reservation width follows the clock count and display scaling. A minimum area remains for Start and application buttons; clocks rotate in groups if needed. Changing modes, removing all taskbar clocks, or quitting releases the reservation. The original layout is also restored if the World Clock process exits unexpectedly. Settings reports a connection or write-permission problem when reservation cannot be activated; clocks then use the hiding behavior until the issue is corrected or another mode is selected.

**Shade background behind text** remains a separate switch, enabled by default. It darkens the panel under light text or lightens it under dark text. The backdrop has soft edges and a short fade when its state changes. Disable it for transparent text alone.

Both settings apply to all taskbar clocks. Choose **Save** to apply and remember them. The old hiding checkbox automatically migrates to Hide or Show over app buttons.

## Rendering

Clock pixels are rendered on the CPU with Windows GDI. Digital cards reuse cached pixels while their content, style, and DPI stay the same. A visible sweeping second hand uses up to 20 timed frames per second; other clocks update at second or minute boundaries. Hidden desktop clocks skip timed rendering, and the fast timer stops when no visible clock needs it. Taskbar groups that already fit do not trigger six-second repaints.

## Settings and portability

Only the executable needs to be copied. The first two placement modes write no files beside it. Reserve space extracts its embedded module into `.WorldClock-taskbar` beside the executable and needs a writable executable folder. World Clock requires no administrator rights. Settings are per Windows user at:

```text
HKEY_CURRENT_USER\Software\WorldClock
```

Settings stay on the computer when you move the executable elsewhere. **Start with Windows** adds the executable path to the current user's `Software\Microsoft\Windows\CurrentVersion\Run` key as `WorldClock`. After moving the executable, turn startup off and back on from its new location.

Time-zone conversion uses the Windows time-zone database, including daylight saving and fractional offsets. Keep Windows time and time-zone updates current.

## Updates

World Clock checks the latest stable release in its GitHub repository when it starts. Open **Updates…** in Settings or the tray menu and disable **Check for updates on startup** to stop automatic checks. This preference saves immediately. Manual checking remains available, and clocks work offline.

The Updates window shows the installed version, the check result, available version, download progress, and recoverable failures. Choose **Check for updates** for a manual check. Startup checks notify only when a newer version is available; ordinary no-update and transient failure results stay quiet.

Choose **Download and install** to download and verify the offered release. The updater downloads the exact `WorldClock-vVERSION.exe` and checks its SHA-256 against `SHA256SUMS.txt`, saves your settings, then replaces the portable executable and restarts. Unsaved invalid clock edits must be corrected before installation can proceed. The installed executable keeps its name and location so startup entries keep working. Language and theme preferences survive the upgrade.

The executable folder must be writable. Incomplete downloads, checksum errors, and network failures leave the current executable intact. Retry from Updates after correcting the problem. If a replacement cannot start, the update helper restores the previous executable. Versions before 0.4.1 require one manual download to gain in-app updates.
