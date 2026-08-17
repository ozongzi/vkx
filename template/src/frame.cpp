// 第九步：每帧一份的资源。
//
// CPU 录制命令和 GPU 执行命令是并行的。CPU 在录第 N+1 帧的时候，GPU 很可能
// 还在执行第 N 帧。两帧共用同一个命令缓冲的话，CPU 就会往一个 GPU 正在读的
// 缓冲里写东西。
//
// 因此备 kFramesInFlight 套资源轮流使用：录第 N 帧用第 N%2 套，录第 N+1 帧
// 用第 (N+1)%2 套。轮回到第 N 套时 GPU 通常已经画完，每套另配一个栅栏来
// 确认这一点。
#include "app.h"
#include "error.h"

// 建每帧一份的资源：命令缓冲、信号量、栅栏。
//
// 信号量和栅栏都是同步原语，区别在于谁在等：
//   信号量（VkSemaphore）  GPU 内部排队用，CPU 看不见它的状态
//   栅栏（VkFence）        给 CPU 等 GPU 用，CPU 可以查询、可以阻塞等待
bool Application::createFrameResources()
{
    // 命令池是命令缓冲的内存来源。一个池只能被一个线程同时使用，
    // 多线程录制时要一个线程一个池。
    VkCommandPoolCreateInfo poolInfo{};
    poolInfo.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO;
    // 这个标志允许单独重置池里的某一个缓冲（vkResetCommandBuffer）。
    // 不加的话只能整池一起重置，而每帧要重置的只是当前这一个。
    poolInfo.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
    poolInfo.queueFamilyIndex = queueFamily_;   // 录出来的命令只能提交给这个族的队列
    VKX_CHECK(vkCreateCommandPool(device_, &poolInfo, nullptr, &commandPool_));

    VkCommandBufferAllocateInfo allocInfo{};
    allocInfo.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
    allocInfo.commandPool = commandPool_;
    // PRIMARY 可以直接提交给队列；SECONDARY 只能被 primary 调用，
    // 多线程分工录制时才用得上。
    allocInfo.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    allocInfo.commandBufferCount = kFramesInFlight;
    // 一次分配 kFramesInFlight 个，直接填进数组。
    // 命令缓冲不需要单独销毁，销毁命令池时会一起没（见 ~Application()）。
    VKX_CHECK(vkAllocateCommandBuffers(device_, &allocInfo, commandBuffers_));

    for (uint32_t i = 0; i < kFramesInFlight; ++i) {
        // imageAvailable_：交换链图像取到手了，可以开始往上面画。
        // 这是 GPU 内部的等待——提交命令时挂上它，GPU 会自己等。
        VkSemaphoreCreateInfo semaphoreInfo{};
        semaphoreInfo.sType = VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO;
        VKX_CHECK(vkCreateSemaphore(device_, &semaphoreInfo, nullptr, &imageAvailable_[i]));

        // inFlight_：这一套资源上一次提交的活干完了没有。这个要 CPU 来等。
        //
        // 建成「已触发」状态：第一帧时这套资源还没提交过任何东西，栅栏若是
        // 未触发的，drawFrame() 开头那句 vkWaitForFences 会一直等下去。
        VkFenceCreateInfo fenceInfo{};
        fenceInfo.sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO;
        fenceInfo.flags = VK_FENCE_CREATE_SIGNALED_BIT;
        VKX_CHECK(vkCreateFence(device_, &fenceInfo, nullptr, &inFlight_[i]));
    }

    return true;
}
