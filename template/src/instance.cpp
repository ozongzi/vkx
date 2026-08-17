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

bool Application::createInstance()
{
    // 先问驱动支持哪些实例扩展，后面按需挑。
    // 要一个驱动不支持的扩展，vkCreateInstance 会直接失败，所以要先查再要。
    uint32_t availableCount = 0;
    vkEnumerateInstanceExtensionProperties(nullptr, &availableCount, nullptr);
    std::vector<VkExtensionProperties> available(availableCount);
    vkEnumerateInstanceExtensionProperties(nullptr, &availableCount, available.data());

    // SDL 知道当前平台开窗口需要哪些扩展（VK_KHR_surface + 平台专用那个，
    // 比如 Windows 上是 VK_KHR_win32_surface）。这些是必需的，不是可选的。
    uint32_t sdlCount = 0;
    const char* const* sdlExtensions = SDL_Vulkan_GetInstanceExtensions(&sdlCount);
    if (sdlExtensions == nullptr) {
        reportError("SDL_Vulkan_GetInstanceExtensions 失败: %s", SDL_GetError());
        return false;
    }

    std::vector<const char*> extensions(sdlExtensions, sdlExtensions + sdlCount);
    VkInstanceCreateFlags flags = 0;

    // Apple 平台的 MoltenVK 是 portability 实现（它把 Vulkan 翻译成 Metal，
    // 不是 100% 完整的 Vulkan）。默认情况下这类实现不会被枚举出来，
    // 必须打开这个扩展和标志，才能在 macOS / iOS 上看到显卡。
    if (hasExtension(available, VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME)) {
        extensions.push_back(VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME);
        flags |= VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR;
    }

    // 还需要别的实例扩展，在这里 extensions.push_back(...)。

    std::vector<const char*> layers;
#if VKX_DEBUG
    // Debug 构建下挂上校验层。它会在每次 Vulkan 调用前后检查参数是否合法、
    // 对象生命周期是否正确、同步是否遗漏，发现问题直接打印出来。
    // Release 构建里整段不编译，没有运行期开销。
    uint32_t layerCount = 0;
    vkEnumerateInstanceLayerProperties(&layerCount, nullptr);
    std::vector<VkLayerProperties> layerProps(layerCount);
    vkEnumerateInstanceLayerProperties(&layerCount, layerProps.data());

    const bool hasValidation = std::any_of(
        layerProps.begin(), layerProps.end(), [](const VkLayerProperties& l) {
            return SDL_strcmp(l.layerName, "VK_LAYER_KHRONOS_validation") == 0;
        });
    // 光有层还不够：层要把消息交出来，得靠 debug utils 这个扩展。两者缺一不可。
    const bool hasDebugUtils = hasExtension(available, VK_EXT_DEBUG_UTILS_EXTENSION_NAME);

    if (hasValidation && hasDebugUtils) {
        layers.push_back("VK_LAYER_KHRONOS_validation");
        extensions.push_back(VK_EXT_DEBUG_UTILS_EXTENSION_NAME);
        // 记下来给 debug.cpp 用：没挂上层就别去建 messenger。
        validationEnabled_ = true;
    } else {
        SDL_LogWarn(SDL_LOG_CATEGORY_GPU, "校验层不可用，跳过（装了 Vulkan SDK 就会有）");
    }
#endif

    // apiVersion 声明本程序按哪一版规范写的。这里用 1.3，
    // 因为下面要用它的 dynamic rendering 和 synchronization2。
    // 其余几个字段只是给驱动看的元信息，某些驱动会按程序名做针对性优化。
    VkApplicationInfo appInfo{};
    appInfo.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    appInfo.pApplicationName = "{{PROJECT_NAME}}";
    appInfo.applicationVersion = VK_MAKE_VERSION(0, 1, 0);
    appInfo.pEngineName = "vkx";
    appInfo.engineVersion = VK_MAKE_VERSION(0, 1, 0);
    appInfo.apiVersion = VK_API_VERSION_1_3;

    VkInstanceCreateInfo createInfo{};
    createInfo.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    createInfo.flags = flags;
    createInfo.pApplicationInfo = &appInfo;
    createInfo.enabledExtensionCount = static_cast<uint32_t>(extensions.size());
    createInfo.ppEnabledExtensionNames = extensions.data();
    createInfo.enabledLayerCount = static_cast<uint32_t>(layers.size());
    createInfo.ppEnabledLayerNames = layers.data();

    VkResult result = vkCreateInstance(&createInfo, nullptr, &instance_);
    if (result == VK_ERROR_INCOMPATIBLE_DRIVER) {
        // 这个返回值最常见的原因就是驱动太老，单独给一句人话提示。
        reportError("驱动不支持 Vulkan 1.3。请更新显卡驱动后重试。");
        return false;
    }
    VKX_CHECK(result);

#if !defined(VKX_STATIC_VULKAN)
    // 实例级函数（vkEnumeratePhysicalDevices 等）到这一步才能取到地址：
    // 它们的实现依赖于实例启用了哪些扩展和层，所以必须先有实例。
    volkLoadInstance(instance_);
#endif

    return true;
}
