// 第二步：从所有显卡里挑一块能用的，然后建出逻辑设备和队列。
#include "app.h"
#include "error.h"

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
