// World Clock's small XAML diagnostics client. No code patches or symbol hooks.
// All changes belong to the primary taskbar's UI thread and are reversible.
#include <windows.h>
#include <ocidl.h>
#include <xamlom.h>
#undef GetCurrentTime
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.Xaml.h>
#include <winrt/Windows.UI.Xaml.Media.h>
#include <atomic>
#include <memory>
#include <string>
#include <cmath>

using namespace winrt;
using namespace Windows::UI::Xaml;
namespace {
constexpr CLSID TapId{0xa7184f54, 0x4ca8, 0x436c, {0x85,0x5a,0x84,0x74,0xcb,0x5c,0x36,0x82}};
constexpr wchar_t ControlClass[] = L"WorldClock.TaskbarReservation.v6";
constexpr UINT ResetOwner = WM_APP + 41;
constexpr UINT Query = WM_APP + 42;
constexpr UINT Reapply = WM_APP + 43;
constexpr ULONG_PTR Protocol = 0x57435231;
HMODULE module;
struct Command { DWORD size; DWORD owner; int pixels; int available; UINT dpi; };
struct Watch { HANDLE process; HANDLE stop; HWND window; DWORD owner; };

DWORD WINAPI watch_owner(void* parameter) {
    std::unique_ptr<Watch> watch(static_cast<Watch*>(parameter));
    HANDLE events[]{watch->stop, watch->process};
    if (WaitForMultipleObjects(2, events, FALSE, INFINITE) == WAIT_OBJECT_0 + 1) {
        PostMessageW(watch->window, ResetOwner, watch->owner, 0);
    }
    CloseHandle(watch->process);
    CloseHandle(watch->stop);
    return 0;
}

FrameworkElement find_background(DependencyObject const& root, int depth = 0) {
    if (!root || depth > 5) return nullptr;
    auto element = root.try_as<FrameworkElement>();
    if (element && get_class_name(element) == L"Taskbar.TaskbarBackground") return element;
    auto count = Media::VisualTreeHelper::GetChildrenCount(root);
    for (int index = 0; index < count && index < 64; ++index) {
        auto found = find_background(Media::VisualTreeHelper::GetChild(root,index), depth+1);
        if (found) return found;
    }
    return nullptr;
}

struct Reservation {
    FrameworkElement frame{nullptr};
    Windows::Foundation::IInspectable original{nullptr};
    bool unset{};
    FrameworkElement background{nullptr};
    Windows::Foundation::IInspectable background_original{nullptr};
    bool background_unset{};
    HWND window{};
    DWORD owner{};
    HANDLE stop{};
    double applied{};
    unsigned changes{};
    Command desired{};
    InstanceHandle visual_handle{};
    void reset(bool forget = true) noexcept {
        try {
            if (frame && original) {
                if (unset) frame.ClearValue(FrameworkElement::MarginProperty()); else frame.SetValue(FrameworkElement::MarginProperty(), original);
                ++changes;
            }
        } catch (...) {}
        try {
            if (background && background_original) {
                if (background_unset) background.ClearValue(FrameworkElement::MarginProperty()); else background.SetValue(FrameworkElement::MarginProperty(), background_original);
                ++changes;
            }
        } catch (...) {}
        original = nullptr;
        background_original = nullptr;
        background = nullptr;
        applied = 0;
        if (stop) { SetEvent(stop); CloseHandle(stop); stop = nullptr; }
        owner = 0;
        if (forget) desired = {};
    }
    bool apply(const Command& command) {
        if (!frame || command.size != sizeof(Command) || command.pixels < 0 ||
            command.pixels > 8192 || command.dpi < 96 || command.dpi > 768 || (command.pixels > 0 && command.pixels >= command.available)) return false;
        if (owner && owner != command.owner) return false;
        if (command.pixels == 0) { reset(); return true; }
        HANDLE process = OpenProcess(SYNCHRONIZE, FALSE, command.owner);
        if (!process) return false;
        if (owner != command.owner || !stop) {
            reset(false);
            owner = command.owner;
            stop = CreateEventW(nullptr, TRUE, FALSE, nullptr);
            HANDLE copy{};
            if (!stop || !DuplicateHandle(GetCurrentProcess(), stop, GetCurrentProcess(), &copy, 0, FALSE, DUPLICATE_SAME_ACCESS)) {
                CloseHandle(process); reset(); return false;
            }
            auto watch = std::make_unique<Watch>(Watch{process, copy, window, owner});
            HANDLE thread = CreateThread(nullptr, 0, watch_owner, watch.get(), 0, nullptr);
            if (!thread) { CloseHandle(copy); CloseHandle(process); reset(); return false; }
            watch.release(); CloseHandle(thread);
        } else { CloseHandle(process); }
        if (!background) background = find_background(frame);
        if (!background) { reset(false); return false; }
        double width = command.pixels * 96.0 / command.dpi;
        if (!original) {
            original = frame.ReadLocalValue(FrameworkElement::MarginProperty());
            unset = original == DependencyProperty::UnsetValue();
            background_original = background.ReadLocalValue(FrameworkElement::MarginProperty());
            background_unset = background_original == DependencyProperty::UnsetValue();
        }
        if (applied != width) {
            auto backdrop_margin = background.Margin();
            backdrop_margin.Right -= width - applied;
            background.Margin(backdrop_margin);
            ++changes;
            auto margin = frame.Margin();
            margin.Right += width - applied;
            frame.Margin(margin);
            applied = width;
            ++changes;
        }
        desired = command;
        return true;
    }
};
thread_local std::unique_ptr<Reservation> reservation;

LRESULT CALLBACK control_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    try {
        if (!reservation) return DefWindowProcW(window, message, wparam, lparam);
        auto& state = *reservation;
        if (message == WM_COPYDATA) {
            auto data = reinterpret_cast<const COPYDATASTRUCT*>(lparam);
            if (!data || data->dwData != Protocol || data->cbData != sizeof(Command) || !data->lpData) return FALSE;
            Command command = *static_cast<const Command*>(data->lpData);
            return state.apply(command);
        }
        if (message == ResetOwner && state.owner == wparam) { state.reset(); return 0; }
        if (message == Reapply) { auto command = state.desired; if (command.pixels > 0) state.apply(command); return 0; }
        if (message == Query) {
            if (!state.frame) return -1;
            switch (wparam) {
                case 0: return std::lround(state.frame.ActualWidth());
                case 1: return std::lround(state.frame.Margin().Right);
                case 2: return std::lround(state.applied);
                case 3: return state.changes;
                case 4: return state.owner;
            }
        }
    } catch (...) {
        if (reservation && (message == WM_COPYDATA || message == Reapply)) reservation->reset(message != Reapply);
        return FALSE;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

struct Tap : implements<Tap, IObjectWithSite, IVisualTreeServiceCallback2> {
    com_ptr<IXamlDiagnostics> diagnostics;
    com_ptr<IUnknown> site;
    HRESULT __stdcall SetSite(IUnknown* value) noexcept {
        try {
            if (!value) { site = nullptr; return S_OK; }
            site.copy_from(value);
            diagnostics = site.as<IXamlDiagnostics>();
            AddRef();
            HANDLE thread = CreateThread(nullptr, 0, [](void* object) -> DWORD {
                auto self = static_cast<Tap*>(object);
                try { self->diagnostics.as<IVisualTreeService3>()->AdviseVisualTreeChange(self); } catch (...) {}
                self->Release();
                return 0;
            }, this, 0, nullptr);
            if (thread) CloseHandle(thread); else Release();
            return S_OK;
        } catch (...) { return to_hresult(); }
    }
    HRESULT __stdcall GetSite(REFIID iid, void** value) noexcept override {
        return site ? site->QueryInterface(iid, value) : E_FAIL;
    }
    HRESULT __stdcall OnVisualTreeChange(ParentChildRelation, VisualElement element, VisualMutationType mutation) noexcept override {
        try {
            if (mutation == Add && element.Type && wcscmp(element.Type, L"Taskbar.TaskbarBackground") == 0 && reservation && reservation->desired.pixels > 0) {
                PostMessageW(reservation->window, Reapply, 0, 0);
            }
            if (mutation == Remove && reservation && reservation->visual_handle == element.Handle) {
                reservation->reset(false); reservation->frame = nullptr; return S_OK;
            }
            if (mutation != Add || !element.Type || wcscmp(element.Type, L"Taskbar.TaskbarFrame") != 0) return S_OK;
            HWND bar = FindWindowW(L"Shell_TrayWnd", nullptr);
            DWORD pid{};
            DWORD thread = GetWindowThreadProcessId(bar, &pid);
            if (pid != GetCurrentProcessId() || thread != GetCurrentThreadId()) return S_OK;
            com_ptr<::IInspectable> inspectable;
            check_hresult(diagnostics->GetIInspectableFromHandle(element.Handle, inspectable.put()));
            auto frame = inspectable.try_as<FrameworkElement>();
            if (!frame) return S_OK;
            if (!reservation) reservation = std::make_unique<Reservation>();
            if (reservation->frame == frame) return S_OK;
            reservation->reset(false);
            reservation->frame = frame;
            reservation->visual_handle = element.Handle;
            if (!reservation->window) {
                WNDCLASSW cls{};
                cls.lpfnWndProc = control_proc;
                cls.hInstance = module;
                cls.lpszClassName = ControlClass;
                RegisterClassW(&cls);
                reservation->window = CreateWindowExW(0, ControlClass, L"", 0, 0,0,0,0, HWND_MESSAGE, nullptr, module, nullptr);
                ChangeWindowMessageFilterEx(reservation->window, WM_COPYDATA, MSGFLT_ALLOW, nullptr);
                ChangeWindowMessageFilterEx(reservation->window, Query, MSGFLT_ALLOW, nullptr);
            }
            if (reservation->desired.pixels > 0) PostMessageW(reservation->window, Reapply, 0, 0);
        } catch (...) {}
        return S_OK;
    }
    HRESULT __stdcall OnElementStateChanged(InstanceHandle, VisualElementState, LPCWSTR) noexcept override { return S_OK; }
};

struct Factory : implements<Factory, IClassFactory> {
    HRESULT __stdcall CreateInstance(IUnknown* outer, REFIID iid, void** value) noexcept override {
        if (outer) return CLASS_E_NOAGGREGATION;
        try { return make<Tap>().as(iid, value); } catch (...) { return to_hresult(); }
    }
    HRESULT __stdcall LockServer(BOOL) noexcept override { return S_OK; }
};
}

STDAPI DllGetClassObject(REFCLSID clsid, REFIID iid, void** value) {
    if (clsid != TapId) return CLASS_E_CLASSNOTAVAILABLE;
    try { return make<Factory>().as(iid, value); } catch (...) { return to_hresult(); }
}
STDAPI DllCanUnloadNow() { return S_FALSE; }
extern "C" __declspec(dllexport) HRESULT WINAPI WorldClockConnect(DWORD pid, const wchar_t*, const wchar_t* client_path) {
    auto runtime = LoadLibraryExW(L"Windows.UI.Xaml.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!runtime) return HRESULT_FROM_WIN32(GetLastError());
    auto initialize = reinterpret_cast<decltype(&InitializeXamlDiagnosticsEx)>(GetProcAddress(runtime, "InitializeXamlDiagnosticsEx"));
    if (!initialize) return E_NOINTERFACE;
    HRESULT hr = E_FAIL;
    for (int index = 1; index <= 10000; ++index) {
        auto connection = L"VisualDiagConnection" + std::to_wstring(index);
        hr = initialize(connection.c_str(), pid, L"", client_path, TapId, nullptr);
        if (hr != HRESULT_FROM_WIN32(ERROR_NOT_FOUND)) break;
    }
    return hr;
}
BOOL WINAPI DllMain(HMODULE instance, DWORD reason, void*) {
    if (reason == DLL_PROCESS_ATTACH) module = instance;
    return TRUE;
}
