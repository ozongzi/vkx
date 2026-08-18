// 第二步：创建 Vulkan 实例（VkInstance）。
//
// 实例是 Vulkan 的总句柄，一个进程一个。创建它的时候要一次性声明清楚三件事：
//   apiVersion  本程序按哪一版规范写的
//   扩展        要用哪些规范之外的功能（开窗口、portability……）
//   层          要在调用链上插哪些拦截器（校验层就是一个）
//
// 这三样都必须在创建时给全，事后加不了——想多用一个扩展就得重建实例。
#include "app.h"
#include "error.h"

bool Application::create_instance()
{
    // 先问驱动支持哪些实例扩展，后面按需挑。
    // 要一个驱动不支持的扩展，vkCreateInstance 会直接失败，所以要先查再要。
    uint32_t available_count = 0;
    vkEnumerateInstanceExtensionProperties(nullptr, &available_count, nullptr);
    std::vector<VkExtensionProperties> available(available_count);
    vkEnumerateInstanceExtensionProperties(nullptr, &available_count, available.data());

    // SDL 知道当前平台开窗口需要哪些扩展（VK_KHR_surface + 平台专用那个，
    // 比如 Windows 上是 VK_KHR_win32_surface）。这些是必需的，不是可选的。
    uint32_t sdl_count = 0;
    const char* const* sdl_extensions = SDL_Vulkan_GetInstanceExtensions(&sdl_count);
    if (sdl_extensions == nullptr) {
        report_error("SDL_Vulkan_GetInstanceExtensions 失败: %s", SDL_GetError());
        return false;
    }

    std::vector<const char*> extensions(sdl_extensions, sdl_extensions + sdl_count);
    VkInstanceCreateFlags flags = 0;

    // Apple 平台的 MoltenVK 是 portability 实现（它把 Vulkan 翻译成 Metal，
    // 不是 100% 完整的 Vulkan）。默认情况下这类实现不会被枚举出来，
    // 必须打开这个扩展和标志，才能在 macOS / iOS 上看到显卡。
    if (has_extension(available, VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME)) {
        extensions.push_back(VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME);
        flags |= VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR;
    }

    // 广色域：Display P3 之类的色彩空间由这个扩展提供，用它才能让画面用上
    // 现代显示器比 sRGB 多出来的那一圈颜色（见 swapchain.cpp 挑格式那段）。
    // 它是可选的，驱动没有就退回 sRGB。
    if (has_extension(available, VK_EXT_SWAPCHAIN_COLOR_SPACE_EXTENSION_NAME)) {
        extensions.push_back(VK_EXT_SWAPCHAIN_COLOR_SPACE_EXTENSION_NAME);
        // 这个标志必须自己记着，不能靠「没启用扩展就枚举不到」来判断：
        // 有些实现（MoltenVK 就是）即使扩展没启用，也照样会在
        // vkGetPhysicalDeviceSurfaceFormatsKHR 里把 P3 报出来。照着用就违反了规范，
        // 程序还照样能跑，只有校验层会告诉你这件事。
        color_space_ext_enabled = true;
    }

    // 还需要别的实例扩展，在这里 extensions.push_back(...)。

    std::vector<const char*> layers;
#if VKX_DEBUG
    // Debug 构建下挂上校验层。它会在每次 Vulkan 调用前后检查参数是否合法、
    // 对象生命周期是否正确、同步是否遗漏，发现问题直接打印出来。
    // Release 构建里整段不编译，没有运行期开销。
    uint32_t layer_count = 0;
    vkEnumerateInstanceLayerProperties(&layer_count, nullptr);
    std::vector<VkLayerProperties> layer_props(layer_count);
    vkEnumerateInstanceLayerProperties(&layer_count, layer_props.data());

    const bool has_validation =
        std::any_of(layer_props.begin(), layer_props.end(), [](const VkLayerProperties& l) {
            return SDL_strcmp(l.layerName, "VK_LAYER_KHRONOS_validation") == 0;
        });
    // 光有层还不够：层要把消息交出来，得靠 debug utils 这个扩展。两者缺一不可。
    const bool has_debug_utils = has_extension(available, VK_EXT_DEBUG_UTILS_EXTENSION_NAME);

    if (has_validation && has_debug_utils) {
        layers.push_back("VK_LAYER_KHRONOS_validation");
        extensions.push_back(VK_EXT_DEBUG_UTILS_EXTENSION_NAME);
        // 记下来给 debug.cpp 用：没挂上层就别去建 messenger。
        validation_enabled = true;
    } else {
        SDL_LogWarn(SDL_LOG_CATEGORY_GPU, "校验层不可用，跳过。执行 `vkx fetch` 补上 SDK 的 vulkan 组件");
    }
#endif

    // apiVersion 声明本程序按哪一版规范写的。这里用 1.3，
    // 因为下面要用它的 dynamic rendering 和 synchronization2。
    // 其余几个字段只是给驱动看的元信息，某些驱动会按程序名做针对性优化。
    VkApplicationInfo app_info{};
    app_info.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    app_info.pApplicationName = "{{PROJECT_NAME}}";
    app_info.applicationVersion = VK_MAKE_VERSION(0, 1, 0);
    app_info.pEngineName = "vkx";
    app_info.engineVersion = VK_MAKE_VERSION(0, 1, 0);
    app_info.apiVersion = VK_API_VERSION_1_3;

    VkInstanceCreateInfo create_info{};
    create_info.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    create_info.flags = flags;
    create_info.pApplicationInfo = &app_info;
    create_info.enabledExtensionCount = static_cast<uint32_t>(extensions.size());
    create_info.ppEnabledExtensionNames = extensions.data();
    create_info.enabledLayerCount = static_cast<uint32_t>(layers.size());
    create_info.ppEnabledLayerNames = layers.data();

    VkResult result = vkCreateInstance(&create_info, nullptr, &instance);
    if (result == VK_ERROR_INCOMPATIBLE_DRIVER) {
        // 这个返回值最常见的原因就是驱动太老，单独给一句人话提示。
        report_error("驱动不支持 Vulkan 1.3。请更新显卡驱动后重试。");
        return false;
    }
    VKX_CHECK(result);

#if !defined(VKX_STATIC_VULKAN)
    // 实例级函数（vkEnumeratePhysicalDevices 等）到这一步才能取到地址：
    // 它们的实现依赖于实例启用了哪些扩展和层，所以必须先有实例。
    volkLoadInstance(instance);
#endif

    return true;
}
