// 第一步：创建 Vulkan 实例，并把 SDL 窗口接成 Vulkan 的表面。
#include "app.h"
#include "error.h"

#if VKX_DEBUG
#if VKX_DEBUG
namespace {

// 校验层的消息出口。Debug 构建里，用错 Vulkan API 的信息会从这里打印出来。
VKAPI_ATTR VkBool32 VKAPI_CALL debugCallback(VkDebugUtilsMessageSeverityFlagBitsEXT severity,
                                             VkDebugUtilsMessageTypeFlagsEXT,
                                             const VkDebugUtilsMessengerCallbackDataEXT* data,
                                             void*)
{
    if (severity >= VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT) {
        SDL_LogWarn(SDL_LOG_CATEGORY_GPU, "[validation] %s", data->pMessage);
    }
    return VK_FALSE;
}

}  // namespace
#endif
#endif

// 创建 VkInstance：声明要用的 API 版本、实例扩展和校验层。
bool Application::createInstance()
{
    // 先问驱动支持哪些实例扩展，后面按需挑。
    uint32_t availableCount = 0;
    vkEnumerateInstanceExtensionProperties(nullptr, &availableCount, nullptr);
    std::vector<VkExtensionProperties> available(availableCount);
    vkEnumerateInstanceExtensionProperties(nullptr, &availableCount, available.data());

    // SDL 知道当前平台开窗口需要哪些扩展（VK_KHR_surface + 平台专用那个）。
    uint32_t sdlCount = 0;
    const char* const* sdlExtensions = SDL_Vulkan_GetInstanceExtensions(&sdlCount);
    if (sdlExtensions == nullptr) {
        reportError("SDL_Vulkan_GetInstanceExtensions 失败: %s", SDL_GetError());
        return false;
    }

    std::vector<const char*> extensions(sdlExtensions, sdlExtensions + sdlCount);
    VkInstanceCreateFlags flags = 0;

    // Apple 平台的 MoltenVK 是 portability 实现。打开这个扩展和标志，
    // 才能在 macOS / iOS 上枚举到显卡。
    if (hasExtension(available, VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME)) {
        extensions.push_back(VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME);
        flags |= VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR;
    }

    // 还需要别的实例扩展，在这里 extensions.push_back(...)。

    std::vector<const char*> layers;
#if VKX_DEBUG
    // Debug 构建下挂上校验层：用错 API 会直接打印出来，比事后调试省事得多。
    uint32_t layerCount = 0;
    vkEnumerateInstanceLayerProperties(&layerCount, nullptr);
    std::vector<VkLayerProperties> layerProps(layerCount);
    vkEnumerateInstanceLayerProperties(&layerCount, layerProps.data());

    const bool hasValidation = std::any_of(
        layerProps.begin(), layerProps.end(), [](const VkLayerProperties& l) {
            return SDL_strcmp(l.layerName, "VK_LAYER_KHRONOS_validation") == 0;
        });
    const bool hasDebugUtils = hasExtension(available, VK_EXT_DEBUG_UTILS_EXTENSION_NAME);

    if (hasValidation && hasDebugUtils) {
        layers.push_back("VK_LAYER_KHRONOS_validation");
        extensions.push_back(VK_EXT_DEBUG_UTILS_EXTENSION_NAME);
    } else {
        SDL_LogWarn(SDL_LOG_CATEGORY_GPU, "校验层不可用，跳过（装了 Vulkan SDK 就会有）");
    }
#endif

    // apiVersion 声明本程序按哪一版规范写的。这里用 1.3，
    // 因为下面要用它的 dynamic rendering 和 synchronization2。
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
        reportError("驱动不支持 Vulkan 1.3。请更新显卡驱动后重试。");
        return false;
    }
    VKX_CHECK(result);

#if !defined(VKX_STATIC_VULKAN)
    // 实例级函数（vkEnumeratePhysicalDevices 等）到这一步才能取到地址。
    volkLoadInstance(instance_);
#endif

#if VKX_DEBUG
    // 把上面那个 debugCallback 注册给校验层。
    if (!layers.empty()) {
        VkDebugUtilsMessengerCreateInfoEXT messengerInfo{};
        messengerInfo.sType = VK_STRUCTURE_TYPE_DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT;
        messengerInfo.messageSeverity = VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT
                                      | VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT;
        messengerInfo.messageType = VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT
                                  | VK_DEBUG_UTILS_MESSAGE_TYPE_VALIDATION_BIT_EXT
                                  | VK_DEBUG_UTILS_MESSAGE_TYPE_PERFORMANCE_BIT_EXT;
        messengerInfo.pfnUserCallback = debugCallback;
        vkCreateDebugUtilsMessengerEXT(instance_, &messengerInfo, nullptr, &debugMessenger_);
    }
#endif

    return true;
}

// 把 SDL 窗口包成 VkSurfaceKHR，之后交换链才能往这个窗口上呈现。
bool Application::createSurface()
{
    if (!SDL_Vulkan_CreateSurface(window_, instance_, nullptr, &surface_)) {
        reportError("SDL_Vulkan_CreateSurface 失败: %s", SDL_GetError());
        return false;
    }
    return true;
}
