// 第四步：表面（VkSurfaceKHR）——窗口在 Vulkan 世界里的代表。
//
// Vulkan 本身不知道「窗口」是什么，各平台开窗口的 API 也完全不同
// （Win32 的 HWND、X11 的 Window、Cocoa 的 CAMetalLayer、Android 的
// ANativeWindow……）。VkSurfaceKHR 就是把这些统一成一个 Vulkan 句柄，
// 之后交换链只认它，不必再关心底下是哪个平台。
//
// 这一步也是「挑显卡」的前提：能不能往这个窗口上呈现，是显卡队列族的属性，
// 得拿着 surface 去问（见 device.cpp 里的 vkGetPhysicalDeviceSurfaceSupportKHR）。
#include "app.h"
#include "error.h"

bool Application::create_surface()
{
    // 平台差异由 SDL 承担，这一句在五个平台上都能用。不用 SDL 的话，
    // Windows 要调 vkCreateWin32SurfaceKHR，macOS 要先建 CAMetalLayer
    // 再调 vkCreateMetalSurfaceEXT，每个平台各写一份。
    if (!SDL_Vulkan_CreateSurface(window, instance, nullptr, &surface)) {
        report_error("SDL_Vulkan_CreateSurface 失败: %s", SDL_GetError());
        return false;
    }
    return true;
}
