// {{PROJECT_NAME}} —— Vulkan + SDL3，在窗口里画一个三角形。
//
// 这个文件负责串起整个流程；每一步的实现见 app.h 顶部的文件清单。
//
// init() 里的创建顺序就是 Vulkan 的依赖顺序：
//   实例 -> 表面 -> 物理设备 -> 逻辑设备 -> 交换链 -> 顶点缓冲 -> 管线 -> 每帧资源
// shutdown() 按相反顺序销毁。

#include "app.h"
#include "error.h"

#include <SDL3/SDL_main.h>

// 初始化 SDL、加载 Vulkan、开窗口，然后按依赖顺序建出全部 Vulkan 资源。
bool Application::init()
{
    if (!SDL_Init(SDL_INIT_VIDEO)) {
        reportError("SDL_Init 失败: %s", SDL_GetError());
        return false;
    }

    // 由 SDL 去找当前平台的 Vulkan 实现：Windows/Linux 是驱动带的 loader，
    // Apple 平台是 MoltenVK，Android 是系统的 libvulkan.so。
    //
    // 它的搜索列表里第一项就是 @executable_path/../Frameworks/libMoltenVK.dylib，
    // 所以 vkx dist 打出来的 .app（MoltenVK 就放在那儿）不需要任何额外处理。
    if (!SDL_Vulkan_LoadLibrary(nullptr)) {
        reportError("找不到 Vulkan 运行时: %s\n\n"
                    "  Windows/Linux: 请更新显卡驱动\n"
                    "  macOS/iOS:     需要 MoltenVK（Vulkan SDK 或 `brew install molten-vk`）\n"
                    "  Android:       设备需支持 Vulkan 1.3",
                    SDL_GetError());
        return false;
    }

    // vkGetInstanceProcAddr 是 Vulkan 的总入口，其它函数都由它派生出来。
    auto getInstanceProcAddr =
        reinterpret_cast<PFN_vkGetInstanceProcAddr>(SDL_Vulkan_GetVkGetInstanceProcAddr());
    if (getInstanceProcAddr == nullptr) {
        reportError("SDL_Vulkan_GetVkGetInstanceProcAddr 失败: %s", SDL_GetError());
        return false;
    }
#if !defined(VKX_STATIC_VULKAN)
    // 把总入口交给 volk，展开出全部全局级函数指针。
    volkInitializeCustom(getInstanceProcAddr);
#endif

    // SDL_WINDOW_VULKAN 让 SDL 建一个能接 Vulkan 表面的窗口。
    window_ = SDL_CreateWindow("{{PROJECT_NAME}}", 1280, 720,
                               SDL_WINDOW_VULKAN | SDL_WINDOW_RESIZABLE | SDL_WINDOW_HIGH_PIXEL_DENSITY);
    if (window_ == nullptr) {
        reportError("SDL_CreateWindow 失败: %s", SDL_GetError());
        return false;
    }

    // 任何一步失败，&& 会短路，init() 直接返回 false。
    return createInstance()
        && createSurface()
        && pickPhysicalDevice()
        && createDevice()
        && createSwapchain()
        && createVertexBuffer()
        && createPipeline()
        && createFrameResources();
}

// 主循环：先把攒下的事件处理完，再画一帧。
void Application::run()
{
    while (running_) {
        SDL_Event event;
        while (SDL_PollEvent(&event)) {
            switch (event.type) {
            case SDL_EVENT_QUIT:            // 关窗口 / 系统要求退出
                running_ = false;
                break;
            case SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED:   // 尺寸变了，交换链要重建
                swapchainDirty_ = true;
                break;
            case SDL_EVENT_KEY_DOWN:
                if (event.key.key == SDLK_ESCAPE) {
                    running_ = false;
                }
                break;
            // 鼠标、触摸、手柄等事件在这里加分支处理。
            default:
                break;
            }
        }

        if (!running_) {
            break;
        }
        // 游戏逻辑的更新（移动、动画、网络同步）放在这一行之前。
        if (!drawFrame()) {
            running_ = false;
        }
    }
}

// 释放所有资源。顺序和创建时相反，销毁前先等 GPU 停下来。
// init() 中途失败也会调用这里，所以每一步都先判空。
void Application::shutdown()
{
    if (device_ != VK_NULL_HANDLE) {
        vkDeviceWaitIdle(device_);   // 等 GPU 用完这些对象再删

        for (uint32_t i = 0; i < kFramesInFlight; ++i) {
            vkDestroySemaphore(device_, imageAvailable_[i], nullptr);
            vkDestroyFence(device_, inFlight_[i], nullptr);
        }
        vkDestroyCommandPool(device_, commandPool_, nullptr);
        vkDestroyPipeline(device_, pipeline_, nullptr);
        vkDestroyPipelineLayout(device_, pipelineLayout_, nullptr);
        vkDestroyBuffer(device_, vertexBuffer_, nullptr);
        vkFreeMemory(device_, vertexMemory_, nullptr);
        // 新建的设备级对象（纹理、采样器、描述符池……）在这里一并销毁。
        destroySwapchain();
        vkDestroyDevice(device_, nullptr);
        device_ = VK_NULL_HANDLE;
    }

    if (instance_ != VK_NULL_HANDLE) {
        if (surface_ != VK_NULL_HANDLE) {
            vkDestroySurfaceKHR(instance_, surface_, nullptr);
        }
#if VKX_DEBUG
        if (debugMessenger_ != VK_NULL_HANDLE) {
            vkDestroyDebugUtilsMessengerEXT(instance_, debugMessenger_, nullptr);
        }
#endif
        vkDestroyInstance(instance_, nullptr);
        instance_ = VK_NULL_HANDLE;
    }

    if (window_ != nullptr) {
        SDL_DestroyWindow(window_);
        window_ = nullptr;
    }
    SDL_Vulkan_UnloadLibrary();
    SDL_Quit();
}

// 程序入口。SDL_main.h 在 Windows、Android、iOS 上会把这个 main
// 接到各平台真正的入口上，所以这里只写标准 main 就够了。
int main(int argc, char* argv[])
{
    (void)argc;
    (void)argv;

    Application app;
    if (!app.init()) {
        app.shutdown();   // 建到一半失败，把已经建好的收干净
        return 1;
    }
    app.run();
    app.shutdown();
    return 0;
}
