// Application：持有全部 Vulkan 对象，串起「创建 -> 渲染 -> 销毁」。
//
// ---------------------------------------------------------------------------
// 文件怎么分的
// ---------------------------------------------------------------------------
// 一个文件负责一件事，顺序和 app.cpp 里 init() 的调用链完全一致，
// 所以 init() 读起来就是这张表：
//
//   main.cpp          程序入口，只有 main()
//   app.cpp           init() 的组装顺序、run() 主循环、~Application() 清理
//   platform.cpp      SDL 初始化、加载 Vulkan 运行时、开窗口
//   instance.cpp      VkInstance：挑实例扩展和校验层
//   debug.cpp         校验层的消息回调（只在 Debug 构建里有内容）
//   surface.cpp       VkSurfaceKHR：把 SDL 窗口接给 Vulkan
//   device.cpp        挑显卡、建逻辑设备和队列
//   swapchain.cpp     交换链及其图像（窗口尺寸一变就整组重建）
//   memory.cpp        找显存类型，任何要分配显存的地方都用它
//   vertex_buffer.cpp 顶点缓冲
//   shader.cpp        把 SPIR-V 字节码包成 VkShaderModule
//   pipeline.cpp      图形管线
//   frame.cpp         每帧一份的资源：命令缓冲、信号量、栅栏
//   draw.cpp          录制一帧的命令，以及一帧真正是怎么提交出去的
//
//   error.h/.cpp      错误上报和 VKX_CHECK 宏
//   vulkan_api.h      Vulkan 头文件从哪来
//   vertex.h          三角形的顶点数据
//
// ---------------------------------------------------------------------------
// 资源是怎么释放的
// ---------------------------------------------------------------------------
// 释放全部写在 ~Application() 里，没有单独的清理函数。C++ 保证对象离开
// 作用域时析构函数一定执行，所以不存在某条 return 路径漏掉清理的情况。
// 析构函数里每一步都先判空，init() 建到一半失败时，已经建好的那部分
// 照样会被正确释放。
//
// 相应地，往这个类里加新资源要改两个地方：init() 的调用链加一句创建，
// ~Application() 加一句销毁。只加前者不会报错，表现为退出时校验层报告
// 「对象还活着」，或者静默泄漏。
//
// 这里的做法是一个析构函数装下全部清理，而不是给每个 VkBuffer、VkPipeline
// 套一个自动释放的包装类。后者的销毁顺序由成员声明顺序倒推得出，而 Vulkan
// 对销毁顺序有硬性要求（例如必须先 vkDeviceWaitIdle），顺序错了同样不报错。
// 现在这份清单是从上到下写死的。
#pragma once

#include "vulkan_api.h"

#include <cstdint>
#include <vector>

// CPU 最多领先 GPU 几帧。2 表示录制下一帧的同时，GPU 还在画上一帧。
constexpr uint32_t kFramesInFlight = 2;

class Application {
public:
    Application() = default;
    ~Application();   // 释放全部资源，见 app.cpp

    bool init();      // 建好窗口和所有 Vulkan 资源；失败返回 false
    void run();       // 主循环：收事件 + 画帧，直到退出

    // 这个类独占一批 Vulkan 句柄，并在析构时释放它们。允许拷贝的话，
    // 两个副本会各自释放同一批句柄，造成重复释放，因此禁用拷贝。
    // 显式声明了拷贝之后，移动构造和移动赋值也会一并被抑制，本工程用不到。
    Application(const Application&) = delete;
    Application& operator=(const Application&) = delete;

private:
    // ---- init() 依次调用这几个，每个对应上面文件清单里的一个 .cpp ----
    bool initPlatform();          // platform.cpp
    bool createInstance();        // instance.cpp
    bool createDebugMessenger();  // debug.cpp
    bool createSurface();         // surface.cpp
    bool pickPhysicalDevice();    // device.cpp
    bool createDevice();          // device.cpp
    bool createSwapchain();       // swapchain.cpp
    bool createVertexBuffer();    // vertex_buffer.cpp
    bool createPipeline();        // pipeline.cpp
    bool createFrameResources();  // frame.cpp
    // 新增一类资源（纹理、uniform 缓冲……）时，在这里加一个 createXxx()，
    // 把它接进 app.cpp 里 init() 的调用链，并在 ~Application() 里加上对应的销毁。

    // ---- 运行期用的 ----
    void destroySwapchain();      // swapchain.cpp
    bool recreateSwapchain();     // swapchain.cpp
    bool drawFrame();             // draw.cpp
    bool recordCommandBuffer(VkCommandBuffer cmd, uint32_t imageIndex);   // draw.cpp

    // ---- 通用零件，跟画什么无关 ----
    bool createShaderModule(const unsigned char* code, size_t size, VkShaderModule* out);  // shader.cpp
    bool findMemoryType(uint32_t typeBits, VkMemoryPropertyFlags wanted, uint32_t* out);   // memory.cpp

    SDL_Window* window_ = nullptr;
    bool running_ = true;
    bool swapchainDirty_ = false;   // 窗口尺寸变了，下一帧前要重建交换链

    // 全局对象：一个进程一份
    VkInstance instance_ = VK_NULL_HANDLE;
    bool validationEnabled_ = false;                 // 实例是否真的挂上了校验层
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
