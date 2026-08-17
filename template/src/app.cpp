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
// 返回 false。出错的那一步已经在自己内部 report_error 过了。
//
// 加新资源在链子末尾加一行，同时在下面的析构函数里加上对应的销毁。
bool Application::init()
{
    return init_platform()          // platform.cpp   SDL、Vulkan 运行时、窗口
        && create_instance()        // instance.cpp   VkInstance
        && create_debug_messenger()  // debug.cpp      校验层的消息出口
        && create_surface()         // surface.cpp    窗口 -> VkSurfaceKHR
        && pick_physical_device()    // device.cpp     挑一块能用的显卡
        && create_device()          // device.cpp     逻辑设备 + 队列
        && create_swapchain()       // swapchain.cpp  交换链及其图像
        && create_vertex_buffer()    // vertex_buffer.cpp
        && create_pipeline()        // pipeline.cpp
        && create_frame_resources(); // frame.cpp      命令缓冲、信号量、栅栏
}

// ---------------------------------------------------------------------------
// 主循环
// ---------------------------------------------------------------------------

// 先把攒下的事件处理完，再画一帧。
//
// 这是「游戏循环」而不是「事件驱动」。SDL_PollEvent 是非阻塞的，没有事件
// 就立刻返回 0，所以每一圈都会走到 draw_frame()，画面每帧都在重画。
// 给循环定节拍的是交换链的垂直同步（见 swapchain.cpp 里的 presentMode），
// 不是 SDL。
void Application::run()
{
    while (running) {
        SDL_Event event;
        while (SDL_PollEvent(&event)) {
            switch (event.type) {
            case SDL_EVENT_QUIT:            // 关窗口 / 系统要求退出
                running = false;
                break;
            case SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED:   // 尺寸变了，交换链要重建
                // 这里只做个记号，不画也不重建。下一次 draw_frame() 开头
                // 看到这个标志才动手——事件处理和绘制保持解耦。
                swapchain_dirty = true;
                break;
            case SDL_EVENT_KEY_DOWN:
                if (event.key.key == SDLK_ESCAPE) {
                    running = false;
                }
                break;
            // 鼠标、触摸、手柄等事件在这里加分支处理。
            default:
                break;
            }
        }

        if (!running) {
            break;
        }
        // 游戏逻辑的更新（移动、动画、网络同步）放在这一行之前。
        if (!draw_frame()) {
            running = false;
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
    if (device != VK_NULL_HANDLE) {
        vkDeviceWaitIdle(device);   // 等 GPU 用完这些对象再删

        for (uint32_t i = 0; i < FRAMES_IN_FLIGHT; ++i) {
            vkDestroySemaphore(device, image_available[i], nullptr);
            vkDestroyFence(device, in_flight[i], nullptr);
        }
        vkDestroyCommandPool(device, command_pool, nullptr);   // 命令缓冲跟着池一起没
        vkDestroyPipeline(device, pipeline, nullptr);
        vkDestroyPipelineLayout(device, pipeline_layout, nullptr);
        vkDestroyBuffer(device, vertex_buffer, nullptr);
        vkFreeMemory(device, vertex_memory, nullptr);
        // 新建的设备级对象（纹理、采样器、描述符池……）在这里一并销毁。
        destroy_swapchain();
        vkDestroyDevice(device, nullptr);
        device = VK_NULL_HANDLE;
    }

    // 实例级的对象：它们不属于任何一块显卡，所以在设备之后才轮到。
    if (instance != VK_NULL_HANDLE) {
        if (surface != VK_NULL_HANDLE) {
            vkDestroySurfaceKHR(instance, surface, nullptr);
        }
#if VKX_DEBUG
        if (debug_messenger != VK_NULL_HANDLE) {
            // 放在 vkDestroyInstance 之前，销毁实例时的报错才还能打印出来。
            vkDestroyDebugUtilsMessengerEXT(instance, debug_messenger, nullptr);
        }
#endif
        vkDestroyInstance(instance, nullptr);
        instance = VK_NULL_HANDLE;
    }

    // 最后收 SDL 那一侧。这两个函数在没初始化过时调用也是安全的，
    // 所以 init() 在第一步就失败时走到这里也不会有问题。
    if (window != nullptr) {
        SDL_DestroyWindow(window);
        window = nullptr;
    }
    SDL_Vulkan_UnloadLibrary();
    SDL_Quit();
}
