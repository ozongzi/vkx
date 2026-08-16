// {{PROJECT_NAME}} —— Vulkan + SDL3，在窗口里画一个三角形。
//
// 全部代码就这一个文件，从上到下依次是：
//
//   1. 顶点数据          kVertices
//   2. 错误处理          reportError / VKX_CHECK
//   3. Application 类    init() 建资源 -> run() 主循环 -> shutdown() 释放
//   4. main()            串起上面三步
//
// init() 里的建资源顺序，就是 Vulkan 的依赖顺序：
//
//   实例 -> 表面 -> 物理设备 -> 逻辑设备 -> 交换链 -> 顶点缓冲 -> 管线 -> 每帧资源

// Vulkan 函数的来源分两种：
//   VKX_STATIC_VULKAN（iOS）—— MoltenVK 静态链接在二进制里，函数直接可调用。
//   其余平台             —— 用 volk 在运行期加载函数指针。
#if defined(VKX_STATIC_VULKAN)
#include <vulkan/vulkan.h>
#else
#include <volk.h>
#endif

#include <SDL3/SDL.h>
#include <SDL3/SDL_main.h>
#include <SDL3/SDL_vulkan.h>

#include <algorithm>
#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iterator>
#include <vector>

#if defined(_WIN32)
#include <io.h>
#else
#include <unistd.h>
#endif

// 这两个头文件由构建系统生成：slangc 把 shaders/triangle.slang 编成 SPIR-V，
// 再转成 C 数组。内容是 kTriangleVertSpv / kTriangleFragSpv 及其长度。
#include "triangle_frag.spv.h"
#include "triangle_vert.spv.h"

namespace {

// CPU 最多领先 GPU 几帧。2 表示录制下一帧的同时，GPU 还在画上一帧。
constexpr uint32_t kFramesInFlight = 2;

// ---------------------------------------------------------------------------
// 顶点数据
// ---------------------------------------------------------------------------

// 一个顶点的内存布局。改这里要同步改 createPipeline() 里的属性描述，
// 以及 shaders/triangle.slang 里的 VertexInput。
struct Vertex {
    float position[2];  // 裁剪空间坐标，范围 [-1, 1]，Y 轴向下
    float color[3];     // 线性 RGB，会在三个顶点之间插值
};

// 三角形的三个顶点。改坐标或颜色，重新 vkx run 就能看到变化。
constexpr Vertex kVertices[] = {
    {{ 0.0f, -0.6f}, {1.0f, 0.0f, 0.0f}},   // 上，红
    {{ 0.6f,  0.6f}, {0.0f, 1.0f, 0.0f}},   // 右下，绿
    {{-0.6f,  0.6f}, {0.0f, 0.0f, 1.0f}},   // 左下，蓝
    // 想画更多三角形，在这里继续加顶点（每三个一组）。
};

// ---------------------------------------------------------------------------
// 错误处理
// ---------------------------------------------------------------------------

// 标准错误是不是接在终端上。
bool stderrIsTerminal()
{
#if defined(_WIN32)
    return _isatty(_fileno(stderr)) != 0;
#else
    return isatty(fileno(stderr)) != 0;
#endif
}

// 报告一条错误：写日志，必要时再弹窗。
//
// 弹窗只在没有终端可看的时候出现（双击运行、手机上）。有终端时不弹，
// 因为模态窗口在 CI 或脚本里没人点，会把进程一直挂住。
void reportError(const char* format, ...)
{
    char message[1024];
    va_list args;
    va_start(args, format);
    SDL_vsnprintf(message, sizeof(message), format, args);
    va_end(args);

    SDL_LogError(SDL_LOG_CATEGORY_APPLICATION, "%s", message);

    if (!stderrIsTerminal()) {
        SDL_ShowSimpleMessageBox(SDL_MESSAGEBOX_ERROR, "{{PROJECT_NAME}}", message, nullptr);
    }
}

// 把 VkResult 转成可读的名字，用于错误信息。
const char* vkResultName(VkResult result)
{
    switch (result) {
    case VK_SUCCESS:                        return "VK_SUCCESS";
    case VK_NOT_READY:                      return "VK_NOT_READY";
    case VK_TIMEOUT:                        return "VK_TIMEOUT";
    case VK_INCOMPLETE:                     return "VK_INCOMPLETE";
    case VK_SUBOPTIMAL_KHR:                 return "VK_SUBOPTIMAL_KHR";
    case VK_ERROR_OUT_OF_HOST_MEMORY:       return "VK_ERROR_OUT_OF_HOST_MEMORY";
    case VK_ERROR_OUT_OF_DEVICE_MEMORY:     return "VK_ERROR_OUT_OF_DEVICE_MEMORY";
    case VK_ERROR_INITIALIZATION_FAILED:    return "VK_ERROR_INITIALIZATION_FAILED";
    case VK_ERROR_DEVICE_LOST:              return "VK_ERROR_DEVICE_LOST";
    case VK_ERROR_LAYER_NOT_PRESENT:        return "VK_ERROR_LAYER_NOT_PRESENT";
    case VK_ERROR_EXTENSION_NOT_PRESENT:    return "VK_ERROR_EXTENSION_NOT_PRESENT";
    case VK_ERROR_FEATURE_NOT_PRESENT:      return "VK_ERROR_FEATURE_NOT_PRESENT";
    case VK_ERROR_INCOMPATIBLE_DRIVER:      return "VK_ERROR_INCOMPATIBLE_DRIVER";
    case VK_ERROR_SURFACE_LOST_KHR:         return "VK_ERROR_SURFACE_LOST_KHR";
    case VK_ERROR_OUT_OF_DATE_KHR:          return "VK_ERROR_OUT_OF_DATE_KHR";
    // 用到新的返回值时，在这里补上对应的名字。
    default:                                return "VK_ERROR_<未知>";
    }
}

// 执行一句 Vulkan 调用；不是 VK_SUCCESS 就报错并让当前函数返回 false。
// 报错信息里带上调用的原文、返回值名字和出错行号。
#define VKX_CHECK(expr)                                                        \
    do {                                                                       \
        VkResult vkxResult = (expr);                                           \
        if (vkxResult != VK_SUCCESS) {                                         \
            reportError("%s\n  返回 %s\n  位置 %s:%d",                          \
                        #expr, vkResultName(vkxResult), __FILE__, __LINE__);   \
            return false;                                                      \
        }                                                                      \
    } while (false)

// 在一组扩展属性里查名字。
bool hasExtension(const std::vector<VkExtensionProperties>& available, const char* name)
{
    return std::any_of(available.begin(), available.end(), [name](const VkExtensionProperties& e) {
        return SDL_strcmp(e.extensionName, name) == 0;
    });
}

#if VKX_DEBUG
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
#endif

// ---------------------------------------------------------------------------
// Application：持有全部 Vulkan 对象，负责创建、渲染、销毁
// ---------------------------------------------------------------------------

class Application {
public:
    bool init();      // 建好窗口和所有 Vulkan 资源
    void run();       // 主循环：收事件 + 画帧，直到退出
    void shutdown();  // 按创建的相反顺序释放；init() 失败时也可安全调用

private:
    // init() 依次调用这几个，每个负责一层资源。
    bool createInstance();
    bool createSurface();
    bool pickPhysicalDevice();
    bool createDevice();
    bool createSwapchain();
    bool createVertexBuffer();
    bool createPipeline();
    bool createFrameResources();
    // 新增一类资源（纹理、uniform 缓冲……）时，在这里加一个 createXxx()，
    // 并把它接进 init() 末尾的调用链。

    void destroySwapchain();
    bool recreateSwapchain();
    bool drawFrame();
    bool recordCommandBuffer(VkCommandBuffer cmd, uint32_t imageIndex);
    bool createShaderModule(const unsigned char* code, size_t size, VkShaderModule* out);
    bool findMemoryType(uint32_t typeBits, VkMemoryPropertyFlags wanted, uint32_t* out);

    SDL_Window* window_ = nullptr;
    bool running_ = true;
    bool swapchainDirty_ = false;   // 窗口尺寸变了，下一帧前要重建交换链

    // 全局对象：一个进程一份
    VkInstance instance_ = VK_NULL_HANDLE;
    VkSurfaceKHR surface_ = VK_NULL_HANDLE;          // 窗口在 Vulkan 里的代表
    VkPhysicalDevice physicalDevice_ = VK_NULL_HANDLE;
    uint32_t queueFamily_ = 0;                       // 同时支持图形和呈现的队列族
    bool needsPortabilitySubset_ = false;
    VkDevice device_ = VK_NULL_HANDLE;               // 逻辑设备，后面所有调用都要它
    VkQueue queue_ = VK_NULL_HANDLE;

    // 交换链：一组等着被画、被显示的图像。窗口尺寸变化时整组重建。
    VkSwapchainKHR swapchain_ = VK_NULL_HANDLE;
    VkFormat swapchainFormat_ = VK_FORMAT_UNDEFINED;
    VkExtent2D swapchainExtent_ = {0, 0};
    std::vector<VkImage> swapchainImages_;
    std::vector<VkImageView> swapchainViews_;
    std::vector<VkSemaphore> renderFinished_;   // 每张交换链图像一个

    // 场景数据
    VkBuffer vertexBuffer_ = VK_NULL_HANDLE;
    VkDeviceMemory vertexMemory_ = VK_NULL_HANDLE;

    // 管线：着色器 + 固定功能状态打包成的一个不可变对象
    VkPipelineLayout pipelineLayout_ = VK_NULL_HANDLE;
    VkPipeline pipeline_ = VK_NULL_HANDLE;

    // 每帧一份，让 CPU 和 GPU 能并行工作
    VkCommandPool commandPool_ = VK_NULL_HANDLE;
    VkCommandBuffer commandBuffers_[kFramesInFlight] = {};
    VkSemaphore imageAvailable_[kFramesInFlight] = {};
    VkFence inFlight_[kFramesInFlight] = {};
    uint32_t frame_ = 0;                        // 当前用第几套每帧资源

#if VKX_DEBUG
    VkDebugUtilsMessengerEXT debugMessenger_ = VK_NULL_HANDLE;
#endif
};

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

// 从所有显卡里挑一块能用的，并记下它可用的队列族。
// 筛选条件：Vulkan 1.3、支持交换链、支持 dynamicRendering 与 synchronization2、
// 有一个同时能图形和呈现的队列族。多块显卡时优先独显。
bool Application::pickPhysicalDevice()
{
    uint32_t deviceCount = 0;
    VKX_CHECK(vkEnumeratePhysicalDevices(instance_, &deviceCount, nullptr));
    if (deviceCount == 0) {
        reportError("没有找到任何支持 Vulkan 的显卡。");
        return false;
    }
    std::vector<VkPhysicalDevice> devices(deviceCount);
    VKX_CHECK(vkEnumeratePhysicalDevices(instance_, &deviceCount, devices.data()));

    int bestScore = -1;
    for (VkPhysicalDevice candidate : devices) {
        VkPhysicalDeviceProperties props{};
        vkGetPhysicalDeviceProperties(candidate, &props);
        if (props.apiVersion < VK_API_VERSION_1_3) {
            continue;
        }

        // 交换链是扩展功能，不是核心的一部分，要单独查。
        uint32_t extCount = 0;
        vkEnumerateDeviceExtensionProperties(candidate, nullptr, &extCount, nullptr);
        std::vector<VkExtensionProperties> exts(extCount);
        vkEnumerateDeviceExtensionProperties(candidate, nullptr, &extCount, exts.data());
        if (!hasExtension(exts, VK_KHR_SWAPCHAIN_EXTENSION_NAME)) {
            continue;
        }

        // 声明支持 1.3 不等于实现了 1.3 的每个特性，用到的要逐个确认。
        VkPhysicalDeviceVulkan13Features features13{};
        features13.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES;
        VkPhysicalDeviceFeatures2 features{};
        features.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2;
        features.pNext = &features13;
        vkGetPhysicalDeviceFeatures2(candidate, &features);
        if (!features13.dynamicRendering || !features13.synchronization2) {
            continue;
        }
        // 需要别的特性（几何着色器、光追……）时，在这里一并检查。

        uint32_t familyCount = 0;
        vkGetPhysicalDeviceQueueFamilyProperties(candidate, &familyCount, nullptr);
        std::vector<VkQueueFamilyProperties> families(familyCount);
        vkGetPhysicalDeviceQueueFamilyProperties(candidate, &familyCount, families.data());

        // 找一个既能画又能呈现的队列族，用它一个就够。
        for (uint32_t i = 0; i < familyCount; ++i) {
            if ((families[i].queueFlags & VK_QUEUE_GRAPHICS_BIT) == 0) {
                continue;
            }
            VkBool32 presentSupported = VK_FALSE;
            vkGetPhysicalDeviceSurfaceSupportKHR(candidate, i, surface_, &presentSupported);
            if (!presentSupported) {
                continue;
            }

            int score = (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) ? 1000 : 100;
            if (score > bestScore) {
                bestScore = score;
                physicalDevice_ = candidate;
                queueFamily_ = i;
                // Apple 平台的设备会带这个扩展，创建逻辑设备时必须一起启用。
                needsPortabilitySubset_ = hasExtension(exts, "VK_KHR_portability_subset");
            }
            break;
        }
    }

    if (physicalDevice_ == VK_NULL_HANDLE) {
        reportError("没有满足要求的显卡。\n\n"
                    "本工程需要 Vulkan 1.3，并支持 dynamicRendering 与 synchronization2。\n"
                    "请先更新显卡驱动；若显卡确实过旧，需改用 VkRenderPass 的写法。");
        return false;
    }

    VkPhysicalDeviceProperties props{};
    vkGetPhysicalDeviceProperties(physicalDevice_, &props);
    SDL_Log("vkx: 使用显卡 %s (Vulkan %u.%u.%u)", props.deviceName,
            VK_API_VERSION_MAJOR(props.apiVersion),
            VK_API_VERSION_MINOR(props.apiVersion),
            VK_API_VERSION_PATCH(props.apiVersion));
    return true;
}

// 创建逻辑设备（VkDevice）和一个队列。
// 逻辑设备是显卡的「使用许可」：只有在这里启用过的扩展和特性才能用。
bool Application::createDevice()
{
    const float priority = 1.0f;
    VkDeviceQueueCreateInfo queueInfo{};
    queueInfo.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO;
    queueInfo.queueFamilyIndex = queueFamily_;
    queueInfo.queueCount = 1;
    queueInfo.pQueuePriorities = &priority;

    std::vector<const char*> deviceExtensions{VK_KHR_SWAPCHAIN_EXTENSION_NAME};
    if (needsPortabilitySubset_) {
        // 规范规定：设备暴露了这个扩展就必须启用它。
        deviceExtensions.push_back("VK_KHR_portability_subset");
    }
    // 需要别的设备扩展，在这里 push_back。

    // 特性默认全关，要用的必须显式打开。
    VkPhysicalDeviceVulkan13Features features13{};
    features13.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES;
    features13.dynamicRendering = VK_TRUE;   // 不用 VkRenderPass 直接开画
    features13.synchronization2 = VK_TRUE;   // 新版屏障和提交结构

    VkPhysicalDeviceFeatures2 features{};
    features.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2;
    features.pNext = &features13;

    VkDeviceCreateInfo deviceInfo{};
    deviceInfo.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO;
    deviceInfo.pNext = &features;
    deviceInfo.queueCreateInfoCount = 1;
    deviceInfo.pQueueCreateInfos = &queueInfo;
    deviceInfo.enabledExtensionCount = static_cast<uint32_t>(deviceExtensions.size());
    deviceInfo.ppEnabledExtensionNames = deviceExtensions.data();

    VKX_CHECK(vkCreateDevice(physicalDevice_, &deviceInfo, nullptr, &device_));

#if !defined(VKX_STATIC_VULKAN)
    // 换成这台设备专用的函数指针，调用时少一层 loader 转发。
    volkLoadDevice(device_);
#endif
    vkGetDeviceQueue(device_, queueFamily_, 0, &queue_);
    return true;
}

// 创建交换链，以及每张图像的 image view 和呈现信号量。
// 窗口尺寸一变，这些东西整组作废，由 recreateSwapchain() 重建。
bool Application::createSwapchain()
{
    // caps 给出尺寸范围、图像数量范围、当前旋转方向等约束。
    VkSurfaceCapabilitiesKHR caps{};
    VKX_CHECK(vkGetPhysicalDeviceSurfaceCapabilitiesKHR(physicalDevice_, surface_, &caps));

    uint32_t formatCount = 0;
    VKX_CHECK(vkGetPhysicalDeviceSurfaceFormatsKHR(physicalDevice_, surface_, &formatCount, nullptr));
    std::vector<VkSurfaceFormatKHR> formats(formatCount);
    VKX_CHECK(vkGetPhysicalDeviceSurfaceFormatsKHR(physicalDevice_, surface_, &formatCount, formats.data()));
    if (formats.empty()) {
        reportError("surface 没有可用的像素格式。");
        return false;
    }

    // 优先挑 sRGB 格式：由硬件做伽马转换，颜色才是对的。挑不到就用第一个。
    VkSurfaceFormatKHR chosen = formats[0];
    for (const VkSurfaceFormatKHR& format : formats) {
        if (format.format == VK_FORMAT_B8G8R8A8_SRGB
            && format.colorSpace == VK_COLOR_SPACE_SRGB_NONLINEAR_KHR) {
            chosen = format;
            break;
        }
    }

    // 尺寸通常由 caps.currentExtent 给定；等于 UINT32_MAX 表示交给应用决定。
    VkExtent2D extent = caps.currentExtent;
    if (extent.width == UINT32_MAX) {
        int width = 0;
        int height = 0;
        SDL_GetWindowSizeInPixels(window_, &width, &height);
        extent.width = std::clamp(static_cast<uint32_t>(width),
                                  caps.minImageExtent.width, caps.maxImageExtent.width);
        extent.height = std::clamp(static_cast<uint32_t>(height),
                                   caps.minImageExtent.height, caps.maxImageExtent.height);
    }
    if (extent.width == 0 || extent.height == 0) {
        // 窗口最小化了，这时建不出交换链，等它恢复。
        return true;
    }

    // 比最小值多一张，避免每帧都要等 GPU 交还图像。
    uint32_t imageCount = caps.minImageCount + 1;
    if (caps.maxImageCount > 0 && imageCount > caps.maxImageCount) {
        imageCount = caps.maxImageCount;
    }

    VkSwapchainCreateInfoKHR info{};
    info.sType = VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR;
    info.surface = surface_;
    info.minImageCount = imageCount;
    info.imageFormat = chosen.format;
    info.imageColorSpace = chosen.colorSpace;
    info.imageExtent = extent;
    info.imageArrayLayers = 1;
    info.imageUsage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT;   // 图像只作渲染目标
    info.imageSharingMode = VK_SHARING_MODE_EXCLUSIVE;       // 只有一个队列族用它
    info.preTransform = caps.currentTransform;               // 沿用设备当前的屏幕方向
    info.compositeAlpha = VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR; // 窗口不透明
    info.presentMode = VK_PRESENT_MODE_FIFO_KHR;             // 垂直同步，所有实现都支持
    info.clipped = VK_TRUE;                                  // 被遮住的像素可以不画

    VKX_CHECK(vkCreateSwapchainKHR(device_, &info, nullptr, &swapchain_));
    swapchainFormat_ = chosen.format;
    swapchainExtent_ = extent;

    // 图像由交换链持有，这里只是把句柄取出来，不需要自己销毁。
    uint32_t actualCount = 0;
    VKX_CHECK(vkGetSwapchainImagesKHR(device_, swapchain_, &actualCount, nullptr));
    swapchainImages_.resize(actualCount);
    VKX_CHECK(vkGetSwapchainImagesKHR(device_, swapchain_, &actualCount, swapchainImages_.data()));

    // 渲染时用的不是 VkImage 本身，而是描述其用法的 image view。
    swapchainViews_.resize(actualCount);
    for (uint32_t i = 0; i < actualCount; ++i) {
        VkImageViewCreateInfo viewInfo{};
        viewInfo.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        viewInfo.image = swapchainImages_[i];
        viewInfo.viewType = VK_IMAGE_VIEW_TYPE_2D;
        viewInfo.format = swapchainFormat_;
        viewInfo.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        viewInfo.subresourceRange.levelCount = 1;
        viewInfo.subresourceRange.layerCount = 1;
        VKX_CHECK(vkCreateImageView(device_, &viewInfo, nullptr, &swapchainViews_[i]));
    }

    // 「渲染完成」信号量按图像分配，一张图像一个：
    // 它要一直有效到这张图像下次被取走为止，生命周期跟帧对不上。
    renderFinished_.resize(actualCount);
    for (uint32_t i = 0; i < actualCount; ++i) {
        VkSemaphoreCreateInfo semaphoreInfo{};
        semaphoreInfo.sType = VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO;
        VKX_CHECK(vkCreateSemaphore(device_, &semaphoreInfo, nullptr, &renderFinished_[i]));
    }

    return true;
}

// 在显卡的内存类型里，找一个既被 typeBits 允许、又满足 wanted 标志的。
bool Application::findMemoryType(uint32_t typeBits, VkMemoryPropertyFlags wanted, uint32_t* out)
{
    VkPhysicalDeviceMemoryProperties properties{};
    vkGetPhysicalDeviceMemoryProperties(physicalDevice_, &properties);

    for (uint32_t i = 0; i < properties.memoryTypeCount; ++i) {
        const bool allowed = (typeBits & (1u << i)) != 0;
        const bool matches = (properties.memoryTypes[i].propertyFlags & wanted) == wanted;
        if (allowed && matches) {
            *out = i;
            return true;
        }
    }

    reportError("找不到满足要求的显存类型（flags = 0x%x）", wanted);
    return false;
}

// 建一个顶点缓冲，把 kVertices 拷进去。
// 三步走：创建 buffer（只是个描述）-> 分配显存 -> 绑定并写入。
bool Application::createVertexBuffer()
{
    const VkDeviceSize size = sizeof(kVertices);

    VkBufferCreateInfo bufferInfo{};
    bufferInfo.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO;
    bufferInfo.size = size;
    bufferInfo.usage = VK_BUFFER_USAGE_VERTEX_BUFFER_BIT;
    bufferInfo.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
    VKX_CHECK(vkCreateBuffer(device_, &bufferInfo, nullptr, &vertexBuffer_));

    // 驱动给出这个 buffer 需要多大、可以放在哪些内存类型上。
    VkMemoryRequirements requirements{};
    vkGetBufferMemoryRequirements(device_, vertexBuffer_, &requirements);

    // HOST_VISIBLE：CPU 能映射来写；HOST_COHERENT：写完不用手动 flush。
    // 数据量大起来之后，一般改成 device-local 显存 + 暂存缓冲上传。
    uint32_t memoryType = 0;
    if (!findMemoryType(requirements.memoryTypeBits,
                        VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                        &memoryType)) {
        return false;
    }

    VkMemoryAllocateInfo allocInfo{};
    allocInfo.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
    allocInfo.allocationSize = requirements.size;
    allocInfo.memoryTypeIndex = memoryType;
    VKX_CHECK(vkAllocateMemory(device_, &allocInfo, nullptr, &vertexMemory_));
    VKX_CHECK(vkBindBufferMemory(device_, vertexBuffer_, vertexMemory_, 0));

    // 映射成 CPU 指针，memcpy 进去，再解除映射。
    void* mapped = nullptr;
    VKX_CHECK(vkMapMemory(device_, vertexMemory_, 0, size, 0, &mapped));
    SDL_memcpy(mapped, kVertices, static_cast<size_t>(size));
    vkUnmapMemory(device_, vertexMemory_);
    return true;
}

// 把一段 SPIR-V 字节码包成 VkShaderModule。
bool Application::createShaderModule(const unsigned char* code, size_t size, VkShaderModule* out)
{
    VkShaderModuleCreateInfo info{};
    info.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO;
    info.codeSize = size;
    info.pCode = reinterpret_cast<const uint32_t*>(code);
    VKX_CHECK(vkCreateShaderModule(device_, &info, nullptr, out));
    return true;
}

// 创建图形管线：把着色器、顶点布局和所有固定功能状态一次性固化下来。
// 这个函数很长，因为 Vulkan 要求把每一项状态都写清楚，没有默认值。
bool Application::createPipeline()
{
    // 着色器模块只在创建管线时用到，函数结尾就销毁。
    VkShaderModule vertModule = VK_NULL_HANDLE;
    VkShaderModule fragModule = VK_NULL_HANDLE;
    if (!createShaderModule(kTriangleVertSpv, kTriangleVertSpv_size, &vertModule)
        || !createShaderModule(kTriangleFragSpv, kTriangleFragSpv_size, &fragModule)) {
        return false;
    }

    // 两个可编程阶段。pName 是 SPIR-V 里的入口名，slangc 统一生成为 "main"。
    VkPipelineShaderStageCreateInfo stages[2]{};
    stages[0].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[0].stage = VK_SHADER_STAGE_VERTEX_BIT;
    stages[0].module = vertModule;
    stages[0].pName = "main";
    stages[1].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[1].stage = VK_SHADER_STAGE_FRAGMENT_BIT;
    stages[1].module = fragModule;
    stages[1].pName = "main";

    // binding 描述「一个顶点占多少字节」，attribute 描述「每个字段在哪、是什么格式」。
    // location 要和 shaders/triangle.slang 里 VertexInput 上的 [[vk::location(N)]] 对上。
    VkVertexInputBindingDescription binding{};
    binding.binding = 0;
    binding.stride = sizeof(Vertex);
    binding.inputRate = VK_VERTEX_INPUT_RATE_VERTEX;   // 每个顶点前进一步

    VkVertexInputAttributeDescription attributes[2]{};
    attributes[0].location = 0;
    attributes[0].binding = 0;
    attributes[0].format = VK_FORMAT_R32G32_SFLOAT;      // float2 position
    attributes[0].offset = offsetof(Vertex, position);
    attributes[1].location = 1;
    attributes[1].binding = 0;
    attributes[1].format = VK_FORMAT_R32G32B32_SFLOAT;   // float3 color
    attributes[1].offset = offsetof(Vertex, color);
    // 加新的顶点属性（UV、法线……）：在 Vertex 里加字段，这里加一条 attribute，
    // 把下面的 vertexAttributeDescriptionCount 一起改掉，着色器里也加对应的 location。

    VkPipelineVertexInputStateCreateInfo vertexInput{};
    vertexInput.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO;
    vertexInput.vertexBindingDescriptionCount = 1;
    vertexInput.pVertexBindingDescriptions = &binding;
    vertexInput.vertexAttributeDescriptionCount = 2;
    vertexInput.pVertexAttributeDescriptions = attributes;

    // 顶点怎么组装成图元：每三个顶点一个三角形。
    VkPipelineInputAssemblyStateCreateInfo inputAssembly{};
    inputAssembly.sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO;
    inputAssembly.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;

    // 视口和裁剪矩形的具体数值是动态的（见下面的 dynamicStates），
    // 这里只声明各要一个。
    VkPipelineViewportStateCreateInfo viewportState{};
    viewportState.sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO;
    viewportState.viewportCount = 1;
    viewportState.scissorCount = 1;

    // 光栅化：图元怎么变成像素。
    VkPipelineRasterizationStateCreateInfo rasterization{};
    rasterization.sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO;
    rasterization.polygonMode = VK_POLYGON_MODE_FILL;          // 填充（改 LINE 可看线框）
    rasterization.cullMode = VK_CULL_MODE_NONE;                // 不剔除背面
    rasterization.frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE; // 逆时针为正面
    rasterization.lineWidth = 1.0f;

    // 多重采样抗锯齿，这里关着（每像素一个采样点）。
    VkPipelineMultisampleStateCreateInfo multisample{};
    multisample.sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO;
    multisample.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;

    // 颜色混合：直接覆盖，RGBA 四个通道都写。
    // 做半透明时在这里打开 blendEnable 并配置混合因子。
    VkPipelineColorBlendAttachmentState blendAttachment{};
    blendAttachment.colorWriteMask = VK_COLOR_COMPONENT_R_BIT | VK_COLOR_COMPONENT_G_BIT
                                   | VK_COLOR_COMPONENT_B_BIT | VK_COLOR_COMPONENT_A_BIT;

    VkPipelineColorBlendStateCreateInfo colorBlend{};
    colorBlend.sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO;
    colorBlend.attachmentCount = 1;
    colorBlend.pAttachments = &blendAttachment;

    // 列在这里的状态改成录制命令时再给，窗口缩放就不必重建管线。
    const VkDynamicState dynamicStates[] = {VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR};
    VkPipelineDynamicStateCreateInfo dynamicState{};
    dynamicState.sType = VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO;
    dynamicState.dynamicStateCount = 2;
    dynamicState.pDynamicStates = dynamicStates;

    // 管线布局声明着色器要读哪些外部资源。现在一个都没有，所以是空的。
    // 加 uniform 缓冲、纹理或 push constant 时，就在这里填。
    VkPipelineLayoutCreateInfo layoutInfo{};
    layoutInfo.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO;
    VKX_CHECK(vkCreatePipelineLayout(device_, &layoutInfo, nullptr, &pipelineLayout_));

    // dynamic rendering 下没有 VkRenderPass，改成在这里告诉管线附件格式。
    VkPipelineRenderingCreateInfo renderingInfo{};
    renderingInfo.sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO;
    renderingInfo.colorAttachmentCount = 1;
    renderingInfo.pColorAttachmentFormats = &swapchainFormat_;
    // 加深度缓冲时，depthAttachmentFormat 也在这里填，
    // 并给 pipelineInfo 补一个 pDepthStencilState。

    VkGraphicsPipelineCreateInfo pipelineInfo{};
    pipelineInfo.sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO;
    pipelineInfo.pNext = &renderingInfo;
    pipelineInfo.stageCount = 2;
    pipelineInfo.pStages = stages;
    pipelineInfo.pVertexInputState = &vertexInput;
    pipelineInfo.pInputAssemblyState = &inputAssembly;
    pipelineInfo.pViewportState = &viewportState;
    pipelineInfo.pRasterizationState = &rasterization;
    pipelineInfo.pMultisampleState = &multisample;
    pipelineInfo.pColorBlendState = &colorBlend;
    pipelineInfo.pDynamicState = &dynamicState;
    pipelineInfo.layout = pipelineLayout_;

    VkResult result =
        vkCreateGraphicsPipelines(device_, VK_NULL_HANDLE, 1, &pipelineInfo, nullptr, &pipeline_);

    // 管线一旦建好，着色器模块就没用了。
    vkDestroyShaderModule(device_, vertModule, nullptr);
    vkDestroyShaderModule(device_, fragModule, nullptr);
    VKX_CHECK(result);
    return true;
}

// 建每帧一份的资源：命令缓冲、信号量、栅栏。
// 信号量用于 GPU 内部排队，栅栏用于 CPU 等 GPU。
bool Application::createFrameResources()
{
    VkCommandPoolCreateInfo poolInfo{};
    poolInfo.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO;
    poolInfo.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;  // 允许单个重置后重录
    poolInfo.queueFamilyIndex = queueFamily_;
    VKX_CHECK(vkCreateCommandPool(device_, &poolInfo, nullptr, &commandPool_));

    VkCommandBufferAllocateInfo allocInfo{};
    allocInfo.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
    allocInfo.commandPool = commandPool_;
    allocInfo.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    allocInfo.commandBufferCount = kFramesInFlight;
    VKX_CHECK(vkAllocateCommandBuffers(device_, &allocInfo, commandBuffers_));

    for (uint32_t i = 0; i < kFramesInFlight; ++i) {
        // imageAvailable_：交换链图像取到手了，可以开始画。
        VkSemaphoreCreateInfo semaphoreInfo{};
        semaphoreInfo.sType = VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO;
        VKX_CHECK(vkCreateSemaphore(device_, &semaphoreInfo, nullptr, &imageAvailable_[i]));

        // inFlight_：这一套资源上一次提交的活干完了没有。
        // 建成已触发状态，第一帧才不会卡在等待上。
        VkFenceCreateInfo fenceInfo{};
        fenceInfo.sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO;
        fenceInfo.flags = VK_FENCE_CREATE_SIGNALED_BIT;
        VKX_CHECK(vkCreateFence(device_, &fenceInfo, nullptr, &inFlight_[i]));
    }

    return true;
}

// 销毁交换链及其附属对象。重建交换链和退出时都会走这里。
void Application::destroySwapchain()
{
    for (VkSemaphore semaphore : renderFinished_) {
        vkDestroySemaphore(device_, semaphore, nullptr);
    }
    renderFinished_.clear();

    for (VkImageView view : swapchainViews_) {
        vkDestroyImageView(device_, view, nullptr);
    }
    swapchainViews_.clear();
    swapchainImages_.clear();   // 图像归交换链所有，只清句柄

    if (swapchain_ != VK_NULL_HANDLE) {
        vkDestroySwapchainKHR(device_, swapchain_, nullptr);
        swapchain_ = VK_NULL_HANDLE;
    }
}

// 窗口尺寸变化后重建交换链。先等 GPU 把手上的活干完，再销毁重建。
bool Application::recreateSwapchain()
{
    vkDeviceWaitIdle(device_);
    destroySwapchain();
    swapchainDirty_ = false;
    return createSwapchain();
}

// 录制一帧的命令：
//   转换图像布局 -> 开始渲染 -> 设视口 -> 绑管线和顶点缓冲 -> 画 -> 结束渲染 -> 转成可呈现
bool Application::recordCommandBuffer(VkCommandBuffer cmd, uint32_t imageIndex)
{
    VkCommandBufferBeginInfo beginInfo{};
    beginInfo.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
    beginInfo.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;  // 录一次用一次
    VKX_CHECK(vkBeginCommandBuffer(cmd, &beginInfo));

    // 屏障之一：把图像从「内容未定义」转成「可以当颜色附件写」。
    // 屏障同时描述布局变化和执行/内存依赖，Vulkan 不会自动帮你做。
    VkImageMemoryBarrier2 toAttachment{};
    toAttachment.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2;
    toAttachment.srcStageMask = VK_PIPELINE_STAGE_2_TOP_OF_PIPE_BIT;
    toAttachment.srcAccessMask = 0;
    toAttachment.dstStageMask = VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT;
    toAttachment.dstAccessMask = VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT;
    toAttachment.oldLayout = VK_IMAGE_LAYOUT_UNDEFINED;
    toAttachment.newLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
    toAttachment.srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
    toAttachment.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
    toAttachment.image = swapchainImages_[imageIndex];
    toAttachment.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
    toAttachment.subresourceRange.levelCount = 1;
    toAttachment.subresourceRange.layerCount = 1;

    VkDependencyInfo dependency{};
    dependency.sType = VK_STRUCTURE_TYPE_DEPENDENCY_INFO;
    dependency.imageMemoryBarrierCount = 1;
    dependency.pImageMemoryBarriers = &toAttachment;
    vkCmdPipelineBarrier2(cmd, &dependency);

    // 这一帧往哪张图像上画、开画时怎么处理已有内容、画完是否保留。
    VkRenderingAttachmentInfo colorAttachment{};
    colorAttachment.sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO;
    colorAttachment.imageView = swapchainViews_[imageIndex];
    colorAttachment.imageLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
    colorAttachment.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR;    // 先清屏
    colorAttachment.storeOp = VK_ATTACHMENT_STORE_OP_STORE;  // 画完留下来
    colorAttachment.clearValue.color = {{0.02f, 0.02f, 0.05f, 1.0f}};  // 背景色

    VkRenderingInfo rendering{};
    rendering.sType = VK_STRUCTURE_TYPE_RENDERING_INFO;
    rendering.renderArea.extent = swapchainExtent_;
    rendering.layerCount = 1;
    rendering.colorAttachmentCount = 1;
    rendering.pColorAttachments = &colorAttachment;

    vkCmdBeginRendering(cmd, &rendering);

    // 视口决定画到图像的哪块区域，裁剪矩形之外的像素会被丢弃。
    VkViewport viewport{};
    viewport.width = static_cast<float>(swapchainExtent_.width);
    viewport.height = static_cast<float>(swapchainExtent_.height);
    viewport.maxDepth = 1.0f;
    vkCmdSetViewport(cmd, 0, 1, &viewport);

    VkRect2D scissor{};
    scissor.extent = swapchainExtent_;
    vkCmdSetScissor(cmd, 0, 1, &scissor);

    // 绑管线、绑数据、下达绘制命令。
    vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline_);

    const VkDeviceSize offset = 0;
    vkCmdBindVertexBuffers(cmd, 0, 1, &vertexBuffer_, &offset);
    vkCmdDraw(cmd, static_cast<uint32_t>(std::size(kVertices)), 1, 0, 0);
    // 要画更多东西，就在这里继续绑管线 / 绑缓冲 / vkCmdDraw。

    vkCmdEndRendering(cmd);

    // 屏障之二：转成可呈现布局，交给显示系统。
    // 大部分字段和上一个屏障相同，直接复制再改差异项。
    VkImageMemoryBarrier2 toPresent = toAttachment;
    toPresent.srcStageMask = VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT;
    toPresent.srcAccessMask = VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT;
    toPresent.dstStageMask = VK_PIPELINE_STAGE_2_BOTTOM_OF_PIPE_BIT;
    toPresent.dstAccessMask = 0;
    toPresent.oldLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
    toPresent.newLayout = VK_IMAGE_LAYOUT_PRESENT_SRC_KHR;
    dependency.pImageMemoryBarriers = &toPresent;
    vkCmdPipelineBarrier2(cmd, &dependency);

    VKX_CHECK(vkEndCommandBuffer(cmd));
    return true;
}

// 画一帧：
//   等这套资源空出来 -> 取一张交换链图像 -> 录制命令 -> 提交 -> 呈现
bool Application::drawFrame()
{
    // 交换链还没建好（首帧或最小化恢复），先补上。
    if (swapchain_ == VK_NULL_HANDLE || swapchainDirty_) {
        if (!recreateSwapchain()) {
            return false;
        }
        if (swapchain_ == VK_NULL_HANDLE) {
            SDL_Delay(16);   // 仍然最小化，空转一帧的时间
            return true;
        }
    }

    // 等这一套每帧资源上次提交的命令跑完，才能安全重用它们。
    VKX_CHECK(vkWaitForFences(device_, 1, &inFlight_[frame_], VK_TRUE, UINT64_MAX));

    // 向交换链要一张可以画的图像。函数会立刻返回，图像真正可用时
    // imageAvailable_ 才被触发，所以下面提交时要等这个信号量。
    uint32_t imageIndex = 0;
    VkResult acquired = vkAcquireNextImageKHR(device_, swapchain_, UINT64_MAX,
                                              imageAvailable_[frame_], VK_NULL_HANDLE, &imageIndex);
    if (acquired == VK_ERROR_OUT_OF_DATE_KHR) {
        // 交换链和窗口尺寸对不上了，这帧作废，下一帧重建。
        swapchainDirty_ = true;
        return true;
    }
    if (acquired != VK_SUCCESS && acquired != VK_SUBOPTIMAL_KHR) {
        VKX_CHECK(acquired);
    }

    // 确认这一帧会提交之后再重置栅栏：上面提前 return 的路径不能重置，
    // 否则会留下一个永远等不到触发的栅栏。
    VKX_CHECK(vkResetFences(device_, 1, &inFlight_[frame_]));

    VkCommandBuffer cmd = commandBuffers_[frame_];
    VKX_CHECK(vkResetCommandBuffer(cmd, 0));
    if (!recordCommandBuffer(cmd, imageIndex)) {
        return false;
    }

    // 提交：等 imageAvailable_，跑完命令后触发 renderFinished_ 和栅栏。
    VkSemaphoreSubmitInfo waitInfo{};
    waitInfo.sType = VK_STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO;
    waitInfo.semaphore = imageAvailable_[frame_];
    waitInfo.stageMask = VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT;  // 只有写颜色时才需要等

    VkSemaphoreSubmitInfo signalInfo{};
    signalInfo.sType = VK_STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO;
    signalInfo.semaphore = renderFinished_[imageIndex];
    signalInfo.stageMask = VK_PIPELINE_STAGE_2_ALL_GRAPHICS_BIT;

    VkCommandBufferSubmitInfo cmdInfo{};
    cmdInfo.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_SUBMIT_INFO;
    cmdInfo.commandBuffer = cmd;

    VkSubmitInfo2 submit{};
    submit.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO_2;
    submit.waitSemaphoreInfoCount = 1;
    submit.pWaitSemaphoreInfos = &waitInfo;
    submit.commandBufferInfoCount = 1;
    submit.pCommandBufferInfos = &cmdInfo;
    submit.signalSemaphoreInfoCount = 1;
    submit.pSignalSemaphoreInfos = &signalInfo;

    VKX_CHECK(vkQueueSubmit2(queue_, 1, &submit, inFlight_[frame_]));

    // 呈现：等渲染完成信号量，然后把这张图像送去显示。
    VkPresentInfoKHR present{};
    present.sType = VK_STRUCTURE_TYPE_PRESENT_INFO_KHR;
    present.waitSemaphoreCount = 1;
    present.pWaitSemaphores = &renderFinished_[imageIndex];
    present.swapchainCount = 1;
    present.pSwapchains = &swapchain_;
    present.pImageIndices = &imageIndex;

    VkResult presented = vkQueuePresentKHR(queue_, &present);
    if (presented == VK_ERROR_OUT_OF_DATE_KHR || presented == VK_SUBOPTIMAL_KHR) {
        swapchainDirty_ = true;
    } else if (presented != VK_SUCCESS) {
        VKX_CHECK(presented);
    }

    // 轮到下一套每帧资源。
    frame_ = (frame_ + 1) % kFramesInFlight;
    return true;
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

}  // namespace

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
