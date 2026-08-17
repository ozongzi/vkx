// 第十步：一帧真正是怎么画出来的。
//
// 两个函数，分工是「写清单」和「交清单」：
//   recordCommandBuffer()  把这一帧要做的事写进命令缓冲，此时 GPU 还没动
//   drawFrame()            取图像、录制、提交、呈现，串起一帧的完整流程
#include "app.h"
#include "error.h"
#include "vertex.h"

#include <iterator>

// 录制一帧的命令：
//   转换图像布局 -> 开始渲染 -> 设视口 -> 绑管线和顶点缓冲 -> 画 -> 结束渲染 -> 转成可呈现
//
// 这里的每个 vkCmdXxx 都只是往命令缓冲里追加一条记录，函数返回时 GPU 一条
// 都还没执行。真正开始跑是在 drawFrame() 里 vkQueueSubmit2 之后。
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
    // 这两项在管线里声明成了动态状态，所以数值在这里给，窗口缩放不必重建管线。
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
