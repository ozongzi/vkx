// 第六步：交换链——一组等着被画、被显示的图像。
//
// 窗口尺寸一变，这里的东西整组作废重建，所以创建和销毁都放在这个文件里。
#include "app.h"
#include "color.h"
#include "error.h"

#include <algorithm>

using vkx::Gamut;
using vkx::gamut_name;

// 创建交换链，以及每张图像的 image view 和呈现信号量。
// 窗口尺寸一变，这些东西整组作废，由 recreate_swapchain() 重建。
bool Application::create_swapchain()
{
    // caps 给出尺寸范围、图像数量范围、当前旋转方向等约束。
    VkSurfaceCapabilitiesKHR caps{};
    VKX_CHECK(vkGetPhysicalDeviceSurfaceCapabilitiesKHR(physical_device, surface, &caps));

    uint32_t format_count = 0;
    VKX_CHECK(
        vkGetPhysicalDeviceSurfaceFormatsKHR(physical_device, surface, &format_count, nullptr));
    std::vector<VkSurfaceFormatKHR> formats(format_count);
    VKX_CHECK(vkGetPhysicalDeviceSurfaceFormatsKHR(physical_device, surface, &format_count,
                                                   formats.data()));
    if (formats.empty()) {
        report_error("surface 没有可用的像素格式。");
        return false;
    }

    // 挑格式和色彩空间。只在第一次创建时挑，之后重建交换链一律沿用同一个选择：
    // 管线创建时把附件格式写死了，中途变格式会和管线对不上。
    if (swapchain_format == VK_FORMAT_UNDEFINED) {
        // 候选按优先级从高到低排。两条筛选原则：
        //
        // 一是尽量用广色域。Display P3 比 sRGB 大一圈，青绿一带差别最明显。
        //
        // 二是坚持 _SRGB 后缀的格式。这个后缀的含义是硬件在写入时自动把线性值编码
        // 成 sRGB，也就是说这步转换是免费的，着色器只管输出线性值就行。
        // Display P3 用的传递函数和 sRGB 是同一条（两者差别只在基色），所以
        // B8G8R8A8_SRGB 配 DISPLAY_P3_NONLINEAR 是合法组合，硬件那步照旧免费。
        //
        // 想再往上走（10 位色深、HDR）就得放弃这个便利：Vulkan 没有 10 位的 _SRGB
        // 格式，用 A2B10G10R10_UNORM 就要自己在着色器里做伽马编码。那是另一个话题。
        const struct {
            VkFormat format;
            VkColorSpaceKHR color_space;
            Gamut gamut;
            bool needs_extension;  // 非 sRGB 的色彩空间都来自 VK_EXT_swapchain_colorspace
        } wanted[] = {
            {VK_FORMAT_B8G8R8A8_SRGB, VK_COLOR_SPACE_DISPLAY_P3_NONLINEAR_EXT, Gamut::DisplayP3,
             true},
            {VK_FORMAT_R8G8B8A8_SRGB, VK_COLOR_SPACE_DISPLAY_P3_NONLINEAR_EXT, Gamut::DisplayP3,
             true},
            {VK_FORMAT_B8G8R8A8_SRGB, VK_COLOR_SPACE_SRGB_NONLINEAR_KHR, Gamut::Srgb, false},
            {VK_FORMAT_R8G8B8A8_SRGB, VK_COLOR_SPACE_SRGB_NONLINEAR_KHR, Gamut::Srgb, false},
        };

        // 一个都匹配不上时退回 surface 报的第一个，并按 sRGB 处理颜色——
        // 那时候颜色可能不完全准，但至少画得出来。
        VkSurfaceFormatKHR chosen = formats[0];
        gamut = Gamut::Srgb;
        bool found = false;
        for (const auto& w : wanted) {
            // 光看 surface 报了什么不够：没启用扩展就不能用扩展带来的色彩空间，
            // 哪怕它出现在列表里。
            if (w.needs_extension && !color_space_ext_enabled) {
                continue;
            }
            for (const VkSurfaceFormatKHR& format : formats) {
                if (format.format == w.format && format.colorSpace == w.color_space) {
                    chosen = format;
                    gamut = w.gamut;
                    found = true;
                    break;
                }
            }
            if (found) {
                break;
            }
        }

        swapchain_format = chosen.format;
        swapchain_color_space = chosen.colorSpace;
        SDL_Log("vkx: 输出色域 %s", gamut_name(gamut));
    }

    VkSurfaceFormatKHR chosen{};
    chosen.format = swapchain_format;
    chosen.colorSpace = swapchain_color_space;

    // 尺寸通常由 caps.currentExtent 给定；等于 UINT32_MAX 表示交给应用决定。
    VkExtent2D extent = caps.currentExtent;
    if (extent.width == UINT32_MAX) {
        int width = 0;
        int height = 0;
        SDL_GetWindowSizeInPixels(window, &width, &height);
        extent.width = std::clamp(static_cast<uint32_t>(width), caps.minImageExtent.width,
                                  caps.maxImageExtent.width);
        extent.height = std::clamp(static_cast<uint32_t>(height), caps.minImageExtent.height,
                                   caps.maxImageExtent.height);
    }
    if (extent.width == 0 || extent.height == 0) {
        // 窗口最小化了，这时建不出交换链，等它恢复。
        return true;
    }

    // 比最小值多一张，避免每帧都要等 GPU 交还图像。
    uint32_t image_count = caps.minImageCount + 1;
    if (caps.maxImageCount > 0 && image_count > caps.maxImageCount) {
        image_count = caps.maxImageCount;
    }

    VkSwapchainCreateInfoKHR info{};
    info.sType = VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR;
    info.surface = surface;
    info.minImageCount = image_count;
    info.imageFormat = chosen.format;
    info.imageColorSpace = chosen.colorSpace;
    info.imageExtent = extent;
    info.imageArrayLayers = 1;
    info.imageUsage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT;    // 图像只作渲染目标
    info.imageSharingMode = VK_SHARING_MODE_EXCLUSIVE;        // 只有一个队列族用它
    info.preTransform = caps.currentTransform;                // 沿用设备当前的屏幕方向
    info.compositeAlpha = VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR;  // 窗口不透明
    info.presentMode = VK_PRESENT_MODE_FIFO_KHR;              // 垂直同步，所有实现都支持
    info.clipped = VK_TRUE;                                   // 被遮住的像素可以不画

    VKX_CHECK(vkCreateSwapchainKHR(device, &info, nullptr, &swapchain));
    // 画布到底多少像素，只有这里知道，所以在这里说出来。工程里没有任何写死的
    // 分辨率：窗口尺寸是逻辑点，画布是物理像素，Retina 屏上两者差一个密度系数。
    // 要按像素排版就读 swapchain_extent，别拿窗口尺寸当像素用。
    //
    // 只在尺寸真的变了时才打印。进全屏、拖边框都会连着重建好几次交换链，
    // 每次都打一行的话，日志里全是同一个数字。
    if (extent.width != swapchain_extent.width || extent.height != swapchain_extent.height) {
        int point_width = 0;
        int point_height = 0;
        SDL_GetWindowSize(window, &point_width, &point_height);
        SDL_Log("vkx: 画布 %ux%u 像素（窗口 %dx%d 点，密度 %.2g）", extent.width, extent.height,
                point_width, point_height, static_cast<double>(SDL_GetWindowPixelDensity(window)));
    }
    swapchain_extent = extent;

    // 图像由交换链持有，这里只是把句柄取出来，不需要自己销毁。
    uint32_t actual_count = 0;
    VKX_CHECK(vkGetSwapchainImagesKHR(device, swapchain, &actual_count, nullptr));
    swapchain_images.resize(actual_count);
    VKX_CHECK(vkGetSwapchainImagesKHR(device, swapchain, &actual_count, swapchain_images.data()));

    // 渲染时用的不是 VkImage 本身，而是描述其用法的 image view。
    swapchain_views.resize(actual_count);
    for (uint32_t i = 0; i < actual_count; ++i) {
        VkImageViewCreateInfo view_info{};
        view_info.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        view_info.image = swapchain_images[i];
        view_info.viewType = VK_IMAGE_VIEW_TYPE_2D;
        view_info.format = swapchain_format;
        view_info.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        view_info.subresourceRange.levelCount = 1;
        view_info.subresourceRange.layerCount = 1;
        VKX_CHECK(vkCreateImageView(device, &view_info, nullptr, &swapchain_views[i]));
    }

    // 「渲染完成」信号量按图像分配，一张图像一个：
    // 它要一直有效到这张图像下次被取走为止，生命周期跟帧对不上。
    render_finished.resize(actual_count);
    for (uint32_t i = 0; i < actual_count; ++i) {
        VkSemaphoreCreateInfo semaphore_info{};
        semaphore_info.sType = VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO;
        VKX_CHECK(vkCreateSemaphore(device, &semaphore_info, nullptr, &render_finished[i]));
    }

    return true;
}

// 销毁交换链及其附属对象。重建交换链和退出时都会走这里。
void Application::destroy_swapchain()
{
    for (VkSemaphore semaphore : render_finished) {
        vkDestroySemaphore(device, semaphore, nullptr);
    }
    render_finished.clear();

    for (VkImageView view : swapchain_views) {
        vkDestroyImageView(device, view, nullptr);
    }
    swapchain_views.clear();
    swapchain_images.clear();  // 图像归交换链所有，只清句柄

    if (swapchain != VK_NULL_HANDLE) {
        vkDestroySwapchainKHR(device, swapchain, nullptr);
        swapchain = VK_NULL_HANDLE;
    }
}

// 窗口尺寸变化后重建交换链。先等 GPU 把手上的活干完，再销毁重建。
bool Application::recreate_swapchain()
{
    vkDeviceWaitIdle(device);
    destroy_swapchain();
    swapchain_dirty = false;
    return create_swapchain();
}
