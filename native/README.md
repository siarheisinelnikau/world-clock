# Embedded taskbar reservation module

The portable executable embeds `WorldClock.Taskbar.dll`, built from `taskbar_reservation.cpp` with the Windows SDK and static Microsoft C runtime. It is extracted beside the executable only for Reserve space. No COM registration, external installer, code patch, or symbol hook is used.

The module connects through Microsoft's [XAML diagnostics API](https://learn.microsoft.com/en-us/windows/win32/api/xamlom/nf-xamlom-initializexamldiagnosticsex). A visual-tree callback finds the primary taskbar's `Taskbar.TaskbarFrame` on its owning UI thread. A private message-only window accepts a fixed-size reservation command; changes are applied only when the desired width changes. The original local Margin values, including unset values, are preserved and restored. A compensating margin keeps TaskbarBackground at its original full width while the application-button frame narrows.

A blocking process-handle wait releases the reservation when its owner exits. There is no watchdog timer or idle layout loop. The helper accepts commands from only one live owner at a time. The app's worker serializes setup and reset operations and ignores results belonging to an older placement request. Explorer recreating a taskbar frame re-applies the pending reservation; restarting Explorer causes the client to reconnect to the new process.

The diagnostics runtime retains COM references to its loaded module. `DllCanUnloadNow` intentionally returns `S_FALSE`; do not force-unload the DLL from Explorer. After disabling reservation, it retains no owner wait or active margin override. Its visual-tree callback only checks for taskbar frame creation/removal. The module leaves memory when Explorer exits normally.

The initial integration supports the standard horizontal Windows 11 taskbar. Other shells retain Hide and Show over app buttons. The callback class and TAP CLSID form a versioned protocol pair; update both when changing helper behavior so a running Explorer cannot reuse an older module.

`--diagnose-reservation <absolute-report.json>` exercises the production background controller, measures the real taskbar before/during/after a bounded test reservation, restores the layout, and exits. It records only geometry, counters, and process CPU. `--hold-reservation` is a bounded test-only fixture for verifying owner-exit cleanup. Capture modes never attach to Explorer.
