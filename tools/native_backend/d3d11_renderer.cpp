// Generated, uncapped window for source-only capture experiments.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi1_5.h>
#include <wrl/client.h>

#include <array>
#include <cstdio>
#include <cstring>
#include <stdexcept>
#include <string>
#include <vector>

using Microsoft::WRL::ComPtr;

static void check(HRESULT result, const char* operation) {
    if (FAILED(result)) {
        std::fprintf(stderr, "%s failed: 0x%08lx\n", operation, static_cast<unsigned long>(result));
        throw std::runtime_error(operation);
    }
}

static long long counter() {
    LARGE_INTEGER value;
    QueryPerformanceCounter(&value);
    return value.QuadPart;
}

static LRESULT CALLBACK window_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    if (message == WM_DESTROY) PostQuitMessage(0);
    return DefWindowProcW(window, message, wparam, lparam);
}

static const char shader[] = R"(
cbuffer Scene : register(b0) { uint frame; uint iterations; float width; float height; };
float4 vertex(uint id : SV_VertexID) : SV_Position {
    return float4(float2((id << 1) & 2, id & 2) * float2(2, -2) + float2(-1, 1), 0, 1);
}
float4 pixel(float4 position : SV_Position) : SV_Target {
    float2 uv = position.xy / float2(width, height);
    if (position.y < 16 && position.x < 256) {
        // The leftmost sixteen 16px stripes encode the current frame ID.
        return float4(((frame >> (uint(position.x) / 16)) & 1).xxx, 1);
    }
    float phase = float(frame % 4096) * 0.013;
    float2 wave = uv * 8 + phase;
    [loop] for (uint i = 0; i < iterations; ++i) {
        wave = sin(wave.yx * float2(1.19, 1.43) + uv * 2 + phase);
    }
    return float4(0.5 + 0.5 * wave, 0.5 + 0.5 * sin(phase + uv.x), 1);
})";

static ComPtr<ID3DBlob> compile(const char* entry, const char* profile) {
    ComPtr<ID3DBlob> bytecode, errors;
    const auto hr = D3DCompile(shader, sizeof(shader) - 1, "synthetic scene", nullptr, nullptr,
        entry, profile, D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &bytecode, &errors);
    if (errors) std::fputs(static_cast<const char*>(errors->GetBufferPointer()), stderr);
    check(hr, "compile renderer shader");
    return bytecode;
}

struct Frame {
    long long start;
    long long submit;
    long long returned;
    bool foreground;
    double gpu_ms = -1;
};

struct TimingQuery {
    ComPtr<ID3D11Query> disjoint, begin, end;
    size_t frame = 0;
    bool pending = false;

    void collect(ID3D11DeviceContext* context, std::vector<Frame>& frames) {
        if (!pending) return;
        D3D11_QUERY_DATA_TIMESTAMP_DISJOINT clock;
        const UINT flags = D3D11_ASYNC_GETDATA_DONOTFLUSH;
        auto result = context->GetData(disjoint.Get(), &clock, sizeof(clock), flags);
        check(result, "read GPU timing disjoint query");
        if (result == S_FALSE) return;
        pending = false;
        if (clock.Disjoint || !clock.Frequency) return;
        UINT64 first = 0, last = 0;
        const auto read_begin = context->GetData(begin.Get(), &first, sizeof(first), flags);
        const auto read_end = context->GetData(end.Get(), &last, sizeof(last), flags);
        check(read_begin, "read first GPU timestamp");
        check(read_end, "read last GPU timestamp");
        if (read_begin == S_OK && read_end == S_OK && last >= first)
            frames[frame].gpu_ms = 1000.0 * (last - first) / clock.Frequency;
    }
};

static void run(int seconds, UINT iterations, const wchar_t* output, const wchar_t* display,
                bool activate) {
    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    ComPtr<IDXGIFactory5> factory;
    check(CreateDXGIFactory1(IID_PPV_ARGS(&factory)), "create DXGI factory");
    const POINT origin{};
    const auto primary = MonitorFromPoint(origin, MONITOR_DEFAULTTOPRIMARY);
    ComPtr<IDXGIAdapter1> adapter;
    DXGI_OUTPUT_DESC monitor{};
    for (UINT a = 0; !adapter; ++a) {
        ComPtr<IDXGIAdapter1> candidate;
        const auto adapter_result = factory->EnumAdapters1(a, &candidate);
        if (adapter_result == DXGI_ERROR_NOT_FOUND) break;
        check(adapter_result, "enumerate DXGI adapter");
        for (UINT o = 0;; ++o) {
            ComPtr<IDXGIOutput> port;
            const auto result = candidate->EnumOutputs(o, &port);
            if (result == DXGI_ERROR_NOT_FOUND) break;
            check(result, "enumerate DXGI output");
            check(port->GetDesc(&monitor), "describe DXGI output");
            if (monitor.AttachedToDesktop &&
                (display ? wcscmp(display, monitor.DeviceName) == 0 : monitor.Monitor == primary)) {
                adapter = candidate;
                break;
            }
        }
    }
    if (!adapter) throw std::runtime_error("requested display has no attached DXGI output");
    DXGI_ADAPTER_DESC1 gpu;
    check(adapter->GetDesc1(&gpu), "describe renderer adapter");
    BOOL tearing = FALSE;
    check(factory->CheckFeatureSupport(DXGI_FEATURE_PRESENT_ALLOW_TEARING, &tearing, sizeof(tearing)),
        "check DXGI tearing");
    if (!tearing) throw std::runtime_error("renderer requires uncapped tearing support");

    const auto bounds = monitor.DesktopCoordinates;
    const auto width = static_cast<UINT>(bounds.right - bounds.left);
    const auto height = static_cast<UINT>(bounds.bottom - bounds.top);
    DEVMODEW mode{};
    mode.dmSize = sizeof(mode);
    if (!EnumDisplaySettingsW(monitor.DeviceName, ENUM_CURRENT_SETTINGS, &mode))
        throw std::runtime_error("describe current display mode");
    const auto instance = GetModuleHandleW(nullptr);
    WNDCLASSW window_class{};
    window_class.lpfnWndProc = window_proc;
    window_class.hInstance = instance;
    window_class.lpszClassName = L"ChronobreakD3d11Fixture";
    if (!RegisterClassW(&window_class)) throw std::runtime_error("register renderer window");
    const auto window = CreateWindowExW(WS_EX_TOPMOST, window_class.lpszClassName,
        L"Chronobreak D3D11 benchmark", WS_POPUP, bounds.left, bounds.top, width, height,
        nullptr, nullptr, instance, nullptr);
    if (!window) throw std::runtime_error("create renderer window");

    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    check(D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0, nullptr, 0,
        D3D11_SDK_VERSION, &device, nullptr, &context), "create renderer device");
    const DXGI_SWAP_CHAIN_DESC1 swap_desc{width, height, DXGI_FORMAT_B8G8R8A8_UNORM,
        FALSE, {1, 0}, DXGI_USAGE_RENDER_TARGET_OUTPUT, 3, DXGI_SCALING_STRETCH,
        DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_ALPHA_MODE_IGNORE, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING};
    ComPtr<IDXGISwapChain1> swap;
    check(factory->CreateSwapChainForHwnd(device.Get(), window, &swap_desc, nullptr, nullptr, &swap),
        "create flip-model swap chain");
    check(factory->MakeWindowAssociation(window, DXGI_MWA_NO_ALT_ENTER), "disable Alt+Enter");
    ComPtr<ID3D11Texture2D> back;
    ComPtr<ID3D11RenderTargetView> target;
    check(swap->GetBuffer(0, IID_PPV_ARGS(&back)), "get back buffer");
    check(device->CreateRenderTargetView(back.Get(), nullptr, &target), "create render target");
    const auto vertex_code = compile("vertex", "vs_5_0");
    const auto pixel_code = compile("pixel", "ps_5_0");
    ComPtr<ID3D11VertexShader> vertex;
    ComPtr<ID3D11PixelShader> pixel;
    check(device->CreateVertexShader(vertex_code->GetBufferPointer(), vertex_code->GetBufferSize(),
        nullptr, &vertex), "create vertex shader");
    check(device->CreatePixelShader(pixel_code->GetBufferPointer(), pixel_code->GetBufferSize(),
        nullptr, &pixel), "create pixel shader");
    struct Scene { UINT frame, iterations; float width, height; };
    const D3D11_BUFFER_DESC constants_desc{sizeof(Scene), D3D11_USAGE_DEFAULT,
        D3D11_BIND_CONSTANT_BUFFER, 0, 0, 0};
    ComPtr<ID3D11Buffer> constants;
    check(device->CreateBuffer(&constants_desc, nullptr, &constants), "create scene constants");
    const D3D11_VIEWPORT viewport{0, 0, static_cast<float>(width), static_cast<float>(height), 0, 1};
    context->RSSetViewports(1, &viewport);
    context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    context->VSSetShader(vertex.Get(), nullptr, 0);
    context->PSSetShader(pixel.Get(), nullptr, 0);
    context->PSSetConstantBuffers(0, 1, constants.GetAddressOf());

    std::array<TimingQuery, 8> queries;
    for (auto& query : queries) {
        const D3D11_QUERY_DESC disjoint{D3D11_QUERY_TIMESTAMP_DISJOINT, 0};
        const D3D11_QUERY_DESC stamp{D3D11_QUERY_TIMESTAMP, 0};
        check(device->CreateQuery(&disjoint, &query.disjoint), "create GPU timing query");
        check(device->CreateQuery(&stamp, &query.begin), "create first GPU timestamp");
        check(device->CreateQuery(&stamp, &query.end), "create last GPU timestamp");
    }

    LARGE_INTEGER frequency;
    QueryPerformanceFrequency(&frequency);
    std::vector<Frame> frames;
    frames.reserve(static_cast<size_t>(seconds) * 2000);
    ShowWindow(window, activate ? SW_SHOW : SW_SHOWNOACTIVATE);
    if (activate) SetForegroundWindow(window);
    SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED);
    std::printf("RENDERER_READY pid=%lu hwnd=%p luid=%08lx%08lx size=%ux%u display=%ls refresh=%lu gpu=%ls freq=%lld\n",
        GetCurrentProcessId(), static_cast<void*>(window), static_cast<unsigned long>(gpu.AdapterLuid.HighPart),
        static_cast<unsigned long>(gpu.AdapterLuid.LowPart), width, height, monitor.DeviceName,
        mode.dmDisplayFrequency, gpu.Description,
        frequency.QuadPart);
    std::fflush(stdout);

    const auto deadline = counter() + seconds * frequency.QuadPart;
    while (counter() < deadline) {
        MSG message;
        while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
            if (message.message == WM_QUIT) throw std::runtime_error("renderer closed during measurement");
            DispatchMessageW(&message);
        }
        const auto id = static_cast<UINT>(frames.size());
        const auto start = counter();
        auto& query = queries[(id / 8) % queries.size()];
        if (id % 8 == 0) query.collect(context.Get(), frames);
        const bool sample = id % 8 == 0 && !query.pending;
        if (sample) {
            context->Begin(query.disjoint.Get());
            context->End(query.begin.Get());
        }
        const Scene scene{id, iterations, viewport.Width, viewport.Height};
        context->UpdateSubresource(constants.Get(), 0, nullptr, &scene, 0, 0);
        context->OMSetRenderTargets(1, target.GetAddressOf(), nullptr);
        context->Draw(3, 0);
        if (sample) {
            context->End(query.end.Get());
            context->End(query.disjoint.Get());
            query.frame = id;
            query.pending = true;
        }
        const auto submit = counter();
        const auto result = swap->Present(0, DXGI_PRESENT_ALLOW_TEARING);
        const auto returned = counter();
        check(result, "present synthetic frame");
        if (result != S_OK) throw std::runtime_error("unexpected renderer Present status");
        frames.push_back({start, submit, returned, GetForegroundWindow() == window});
    }
    for (auto& query : queries) query.collect(context.Get(), frames);
    SetThreadExecutionState(ES_CONTINUOUS);
    DestroyWindow(window);
    FILE* file = nullptr;
    if (_wfopen_s(&file, output, L"w") != 0) throw std::runtime_error("open frame trace output");
    std::fprintf(file, "start_qpc,present_qpc,return_qpc,foreground,gpu_ms\n");
    for (const auto& f : frames)
        std::fprintf(file, "%lld,%lld,%lld,%d,%.6f\n", f.start, f.submit, f.returned, f.foreground, f.gpu_ms);
    if (std::fclose(file) != 0) throw std::runtime_error("write frame trace");
}

int wmain(int argc, wchar_t** argv) {
    try {
        if (argc < 4 || argc > 6 || (argc == 6 && wcscmp(argv[5], L"no-activate") != 0))
            throw std::runtime_error("usage: d3d11_renderer SECONDS SHADER_ITERATIONS CSV [DISPLAY|-] [no-activate]");
        const auto duration = std::stoi(argv[1]);
        const auto iterations = std::stoi(argv[2]);
        if (duration < 1 || duration > 1800 || iterations < 1 || iterations > 512)
            throw std::runtime_error("duration must be 1-1800s and shader iterations 1-512");
        const wchar_t* display = argc >= 5 && wcscmp(argv[4], L"-") != 0 ? argv[4] : nullptr;
        run(duration, static_cast<UINT>(iterations), argv[3], display, argc < 6);
    } catch (const std::exception& error) {
        std::fprintf(stderr, "%s\n", error.what());
        return 1;
    }
}
