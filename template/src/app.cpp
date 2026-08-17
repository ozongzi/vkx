// Application 的骨架：怎么组装起来、怎么跑、怎么拆掉。
//
// 这个文件只有三个函数，具体每一步干了什么在各自的 .cpp 里，见 app.h 顶部的清单。
#include "app.h"
#include "error.h"

// ---------------------------------------------------------------------------
// 组装
// ---------------------------------------------------------------------------

// 按依赖顺序把所有东西建出来。
//
// 这个顺序不是随便排的，是 Vulkan 的依赖关系逼出来的：
//   要有窗口，才能建表面；
//   要有表面，才能判断显卡的哪个队列族能往这个窗口上呈现；
//   要有逻辑设备，才能建交换链、缓冲、管线；
//   要有交换链，才知道图像格式，管线才能建（管线要写死附件格式）。
//
// `&&` 会短路：任何一步返回 false，后面的调用都不再执行，init() 直接
// 返回 false。出错的那一步已经在自己内部 reportError 过了。
//
// 加新资源在链子末尾加一行，同时在下面的析构函数里加上对应的销毁。
bool Application::init()
{
    return initPlatform()          // platform.cpp   SDL、Vulkan 运行时、窗口
        && createInstance()        // instance.cpp   VkInstance
        && createDebugMessenger()  // debug.cpp      校验层的消息出口
        && createSurface()         // surface.cpp    窗口 -> VkSurfaceKHR
        && pickPhysicalDevice()    // device.cpp     挑一块能用的显卡
        && createDevice()          // device.cpp     逻辑设备 + 队列
        && createSwapchain()       // swapchain.cpp  交换链及其图像
        && createVertexBuffer()    // vertex_buffer.cpp
        && createPipeline()        // pipeline.cpp
        && createFrameResources(); // frame.cpp      命令缓冲、信号量、栅栏
}

// ---------------------------------------------------------------------------
// 主循环
// ---------------------------------------------------------------------------

// 先把攒下的事件处理完，再画一帧。
//
// 这是「游戏循环」而不是「事件驱动」。SDL_PollEvent 是非阻塞的，没有事件
// 就立刻返回 0，所以每一圈都会走到 drawFrame()，画面每帧都在重画。
// 给循环定节拍的是交换链的垂直同步（见 swapchain.cpp 里的 presentMode），
// 不是 SDL。
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
                // 这里只做个记号，不画也不重建。下一次 drawFrame() 开头
                // 看到这个标志才动手——事件处理和绘制保持解耦。
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

// ---------------------------------------------------------------------------
// 拆卸
// ---------------------------------------------------------------------------

// 释放所有资源，顺序和创建时相反。
//
// 析构函数不需要谁来调用：main() 里的 Application 对象一离开作用域就执行，
// 正常退出和 init() 中途失败走的都是这一条路。
//
// 两处需要说明：
//
// 一是每一步都先判空。init() 可能建到一半就失败（比如显卡不支持 1.3），
// 这时后面那些句柄还是 VK_NULL_HANDLE，直接删会崩。判空之后，这个函数
// 对「建了多少」不敏感，建到哪儿就拆到哪儿。
//
// 二是先 vkDeviceWaitIdle。GPU 可能还在画最后一帧，正在用命令缓冲、管线、
// 顶点缓冲。销毁一个 GPU 正在使用的对象是未定义行为，要先等它停下来。
Application::~Application()
{
    if (device_ != VK_NULL_HANDLE) {
        vkDeviceWaitIdle(device_);   // 等 GPU 用完这些对象再删

        for (uint32_t i = 0; i < kFramesInFlight; ++i) {
            vkDestroySemaphore(device_, imageAvailable_[i], nullptr);
            vkDestroyFence(device_, inFlight_[i], nullptr);
        }
        vkDestroyCommandPool(device_, commandPool_, nullptr);   // 命令缓冲跟着池一起没
        vkDestroyPipeline(device_, pipeline_, nullptr);
        vkDestroyPipelineLayout(device_, pipelineLayout_, nullptr);
        vkDestroyBuffer(device_, vertexBuffer_, nullptr);
        vkFreeMemory(device_, vertexMemory_, nullptr);
        // 新建的设备级对象（纹理、采样器、描述符池……）在这里一并销毁。
        destroySwapchain();
        vkDestroyDevice(device_, nullptr);
        device_ = VK_NULL_HANDLE;
    }

    // 实例级的对象：它们不属于任何一块显卡，所以在设备之后才轮到。
    if (instance_ != VK_NULL_HANDLE) {
        if (surface_ != VK_NULL_HANDLE) {
            vkDestroySurfaceKHR(instance_, surface_, nullptr);
        }
#if VKX_DEBUG
        if (debugMessenger_ != VK_NULL_HANDLE) {
            // 放在 vkDestroyInstance 之前，销毁实例时的报错才还能打印出来。
            vkDestroyDebugUtilsMessengerEXT(instance_, debugMessenger_, nullptr);
        }
#endif
        vkDestroyInstance(instance_, nullptr);
        instance_ = VK_NULL_HANDLE;
    }

    // 最后收 SDL 那一侧。这两个函数在没初始化过时调用也是安全的，
    // 所以 init() 在第一步就失败时走到这里也不会有问题。
    if (window_ != nullptr) {
        SDL_DestroyWindow(window_);
        window_ = nullptr;
    }
    SDL_Vulkan_UnloadLibrary();
    SDL_Quit();
}
