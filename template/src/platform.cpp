// 第一步：平台层。这一步之后才有窗口，才谈得上 Vulkan。
//
// 三件事：把 SDL 拉起来、把当前平台的 Vulkan 实现找出来、开一个窗口。
// 全都是 SDL 的活，一句 Vulkan API 都还没调。
#include "app.h"
#include "error.h"

bool Application::initPlatform()
{
    // 只初始化视频子系统。要用手柄、音频时，在这里按位或上
    // SDL_INIT_GAMEPAD、SDL_INIT_AUDIO。
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
    // 上面那句 LoadLibrary 之后，SDL 就能把这个函数的地址交出来了。
    auto getInstanceProcAddr =
        reinterpret_cast<PFN_vkGetInstanceProcAddr>(SDL_Vulkan_GetVkGetInstanceProcAddr());
    if (getInstanceProcAddr == nullptr) {
        reportError("SDL_Vulkan_GetVkGetInstanceProcAddr 失败: %s", SDL_GetError());
        return false;
    }
#if !defined(VKX_STATIC_VULKAN)
    // 把总入口交给 volk，展开出全部全局级函数指针（vkCreateInstance 等）。
    // iOS 上静态链接了 MoltenVK，函数本来就在二进制里，不需要这一步。
    volkInitializeCustom(getInstanceProcAddr);
#endif

    // SDL_WINDOW_VULKAN         让 SDL 建一个能接 Vulkan 表面的窗口
    // SDL_WINDOW_RESIZABLE      允许拖动边框改大小（交换链会跟着重建）
    // SDL_WINDOW_HIGH_PIXEL_DENSITY  在 Retina 屏上拿到真实像素数的画布，
    //                           而不是被系统放大的模糊图像
    window_ = SDL_CreateWindow("{{PROJECT_NAME}}", 1280, 720,
                               SDL_WINDOW_VULKAN | SDL_WINDOW_RESIZABLE | SDL_WINDOW_HIGH_PIXEL_DENSITY);
    if (window_ == nullptr) {
        reportError("SDL_CreateWindow 失败: %s", SDL_GetError());
        return false;
    }

    return true;
}
