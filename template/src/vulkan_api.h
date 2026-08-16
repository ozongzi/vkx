// Vulkan 头文件从哪来，以及一个到处要用的小工具。
//
// 函数来源分两种：
//   VKX_STATIC_VULKAN（iOS）—— MoltenVK 静态链接在二进制里，函数直接可调用。
//   其余平台             —— 用 volk 在运行期加载函数指针。
#pragma once

// Vulkan 函数的来源分两种：
//   VKX_STATIC_VULKAN（iOS）—— MoltenVK 静态链接在二进制里，函数直接可调用。
//   其余平台             —— 用 volk 在运行期加载函数指针。
#if defined(VKX_STATIC_VULKAN)
#include <vulkan/vulkan.h>
#else
#include <volk.h>
#endif

#include <SDL3/SDL.h>
#include <SDL3/SDL_vulkan.h>

#include <algorithm>
#include <vector>

// 在一组扩展属性里查名字。
inline bool hasExtension(const std::vector<VkExtensionProperties>& available, const char* name)
{
    return std::any_of(available.begin(), available.end(), [name](const VkExtensionProperties& e) {
        return SDL_strcmp(e.extensionName, name) == 0;
    });
}
