// 第一步：平台层。这一步之后才有窗口，才谈得上 Vulkan。
//
// 三件事：把 SDL 拉起来、把当前平台的 Vulkan 实现找出来、开一个窗口。
// 全都是 SDL 的活，一句 Vulkan API 都还没调。
#include "app.h"
#include "error.h"

bool Application::init_platform()
{
    // 只初始化视频子系统。要用手柄、音频时，在这里按位或上
    // SDL_INIT_GAMEPAD、SDL_INIT_AUDIO。
    if (!SDL_Init(SDL_INIT_VIDEO)) {
        report_error("SDL_Init 失败: %s", SDL_GetError());
        return false;
    }

    // SDL 给绝大多数日志分类的默认门槛是 ERROR，只有 APPLICATION 分类是 INFO。
    // 校验层的消息和「校验层不可用」这类提示都走 GPU 分类的 WARN 级别，
    // 不把门槛降下来就一个字也看不到——看起来像是「没有问题」，
    // 实际是「有问题但没人告诉你」。
    SDL_SetLogPriority(SDL_LOG_CATEGORY_GPU, SDL_LOG_PRIORITY_WARN);

    // 由 SDL 去找当前平台的 Vulkan 实现：Windows/Linux 是驱动带的 loader，
    // Apple 平台是 MoltenVK，Android 是系统的 libvulkan.so。
    //
    // 它的搜索列表里第一项就是 @executable_path/../Frameworks/libMoltenVK.dylib，
    // 所以 vkx dist 打出来的 .app（MoltenVK 就放在那儿）不需要任何额外处理。
    if (!SDL_Vulkan_LoadLibrary(nullptr)) {
        report_error(
            "找不到 Vulkan 运行时: %s\n\n"
            "  Windows/Linux: 请更新显卡驱动\n"
            "  macOS/iOS:     需要 MoltenVK（Vulkan SDK 或 `brew install molten-vk`）\n"
            "  Android:       设备需支持 Vulkan 1.3",
            SDL_GetError());
        return false;
    }

    // vkGetInstanceProcAddr 是 Vulkan 的总入口，其它函数都由它派生出来。
    // 上面那句 LoadLibrary 之后，SDL 就能把这个函数的地址交出来了。
    auto get_instance_proc_addr =
        reinterpret_cast<PFN_vkGetInstanceProcAddr>(SDL_Vulkan_GetVkGetInstanceProcAddr());
    if (get_instance_proc_addr == nullptr) {
        report_error("SDL_Vulkan_GetVkGetInstanceProcAddr 失败: %s", SDL_GetError());
        return false;
    }
#if !defined(VKX_STATIC_VULKAN)
    // 把总入口交给 volk，展开出全部全局级函数指针（vkCreateInstance 等）。
    // iOS 上静态链接了 MoltenVK，函数本来就在二进制里，不需要这一步。
    volkInitializeCustom(get_instance_proc_addr);
#endif

    // SDL_WINDOW_VULKAN         让 SDL 建一个能接 Vulkan 表面的窗口
    // SDL_WINDOW_RESIZABLE      允许拖动边框改大小（交换链会跟着重建）
    // SDL_WINDOW_HIGH_PIXEL_DENSITY  在 Retina 屏上拿到真实像素数的画布，
    //                           而不是被系统放大的模糊图像
    // 单独拎出来，是为了让这一行的长度不受工程名影响：工程名是模版占位符，
    // 长短不定，写在一起的话 clang-format 会按名字长度选择不同的折行方式。
    const SDL_WindowFlags flags =
        SDL_WINDOW_VULKAN | SDL_WINDOW_RESIZABLE | SDL_WINDOW_HIGH_PIXEL_DENSITY;
    window = SDL_CreateWindow("{{PROJECT_NAME}}", 1280, 720, flags);
    if (window == nullptr) {
        report_error("SDL_CreateWindow 失败: %s", SDL_GetError());
        return false;
    }

    return true;
}
