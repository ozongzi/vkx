// 第六步：交换链——一组等着被画、被显示的图像。
//
// 窗口尺寸一变，这里的东西整组作废重建，所以创建和销毁都放在这个文件里。
#include "app.h"
#include "error.h"

#include <algorithm>

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
