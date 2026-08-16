// Application：持有全部 Vulkan 对象，串起「创建 -> 渲染 -> 销毁」。
//
// 各个步骤的实现按流程分在不同文件里：
//   main.cpp          入口、初始化顺序、主循环、清理
//   instance.cpp      实例、校验层、窗口表面
//   device.cpp        挑显卡、建逻辑设备
//   swapchain.cpp     交换链及其图像（窗口尺寸一变就整组重建）
//   vertex_buffer.cpp 顶点缓冲和显存
//   pipeline.cpp      着色器模块、图形管线
//   frame.cpp         每帧资源、录制命令、画一帧
#pragma once

#include "vulkan_api.h"

#include <cstdint>
#include <vector>

// CPU 最多领先 GPU 几帧。2 表示录制下一帧的同时，GPU 还在画上一帧。
constexpr uint32_t kFramesInFlight = 2;

class Application {
public:
    bool init();      // 建好窗口和所有 Vulkan 资源
    void run();       // 主循环：收事件 + 画帧，直到退出
    void shutdown();  // 按创建的相反顺序释放；init() 失败时也可安全调用

private:
    // init() 依次调用这几个，每个负责一层资源。
    bool createInstance();
    bool createSurface();
    bool pickPhysicalDevice();
    bool createDevice();
    bool createSwapchain();
    bool createVertexBuffer();
    bool createPipeline();
    bool createFrameResources();
    // 新增一类资源（纹理、uniform 缓冲……）时，在这里加一个 createXxx()，
    // 并把它接进 init() 末尾的调用链。

    void destroySwapchain();
    bool recreateSwapchain();
    bool drawFrame();
    bool recordCommandBuffer(VkCommandBuffer cmd, uint32_t imageIndex);
    bool createShaderModule(const unsigned char* code, size_t size, VkShaderModule* out);
    bool findMemoryType(uint32_t typeBits, VkMemoryPropertyFlags wanted, uint32_t* out);

    SDL_Window* window_ = nullptr;
    bool running_ = true;
    bool swapchainDirty_ = false;   // 窗口尺寸变了，下一帧前要重建交换链

    // 全局对象：一个进程一份
    VkInstance instance_ = VK_NULL_HANDLE;
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
