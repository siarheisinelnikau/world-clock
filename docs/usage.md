# Using World Clock

## Choose your clocks

Open **Settings** from the clock icon in the notification area. Windows may initially put it in the overflow menu (`^`). Launching `WorldClock.exe` again also opens Settings in the running instance.

Add up to 12 clocks, give them useful labels, and reorder them with the arrow buttons. Each clock has independent **On desktop** and **In taskbar** switches. **Local** follows the Windows time zone; it can be hidden from both surfaces but cannot be removed. Other clocks need at least one display destination.

Optional settings include 12-hour time, desktop seconds, always on top, and starting with Windows. Startup is off by default.

## Position the desktop cards

Choose **Save & position clocks…** in Settings, or **Position desktop clocks…** in the tray menu. Drag the cards into place, then double-click to lock them or choose **Lock desktop clocks** from the tray menu. Locking preserves the position and makes clicks and scrolling pass through again.

Desktop cards normally sit behind application windows. **Always on top** keeps them above applications while still passing mouse input through. Tall lists use additional columns.

## Design a card

Select a clock and open **Appearance…**. Changes appear immediately in the preview. Choose **Use style**, then **Save** in Settings to keep them. **Apply this style to all clocks** copies the design to the other clocks.

### Card

Drag the blue handles on the preview edges or corners to resize. Width is 120–600 logical pixels; explicit height is 24–700. **Auto** fits height to the content. Padding, space between cards, and corner radius are configurable. Dimensions scale with Windows display scaling.

### Content

Choose a preset or edit up to eight lines. Templates support `{name}`, `{time}`, `{date}`, `{offset}`, and `{day}`, plus literal text. Lines containing `{time}` use the time font size; other lines use the detail size.

The font picker lists installed Windows font families, including fonts installed for your user account. Scroll the list or type a font name. Reopen Appearance after installing a font to refresh the list.

Drag the time, name, UTC offset, date, or day directly in the preview. A dotted outline marks the selected element, with its coordinates below the preview. **Reset text positions** returns to the template layout. Editing the template or alignment resets custom positions. Press **Esc** during a drag to cancel it.

### Colors

Text, text outline, card background, and card border each have their own color and opacity (0–100%). Text outline and card border also have separate widths (0–4 pixels). Setting background opacity to 0% leaves the other enabled layers visible. **Reset to compact** restores the default style in the editor.

## Taskbar behavior

The extra clocks sit in the free gap between app buttons and the primary taskbar's tray. They use transparent text and pass mouse input through. Top and bottom taskbars use a horizontal row; left and right taskbars use a vertical stack, with time text adjusted to fit the panel width. The native Windows clock stays visible according to your Windows settings. Hiding the native clock does not hide World Clock.

Approximately 200 logical pixels of horizontal space or 140 logical pixels of vertical space are needed for three slots. More selected zones rotate in groups when necessary. If the taskbar is too full for the minimum group, the clocks hide to avoid covering buttons or tray controls. Fullscreen and taskbar auto-hide are respected on every edge. Ordinary maximized windows and Explorer's task switcher keep the clocks visible. Brief accessibility failures retain the last valid layout instead of clearing the clocks. Placement targets the primary Windows taskbar; modified shells may expose a different accessibility structure.

## Settings and portability

Only the executable needs to be copied. Normal use writes no files beside it and requires no administrator rights. Settings are per Windows user at:

```text
HKEY_CURRENT_USER\Software\WorldClock
```

Settings stay on the computer when you move the executable elsewhere. **Start with Windows** adds the executable path to the current user's `Software\Microsoft\Windows\CurrentVersion\Run` key as `WorldClock`. After moving the executable, turn startup off and back on from its new location.

Time-zone conversion uses the Windows time-zone database, including daylight saving and fractional offsets. Keep Windows time and time-zone updates current.
