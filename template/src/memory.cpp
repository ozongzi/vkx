// 通用零件：在显卡上找一块合适的内存。
//
// 这个文件不属于流程里的任何一步，但凡是要分配显存的地方（顶点缓冲、
// uniform 缓冲、纹理……）都得先经过它，所以单独放一份。
#include "app.h"
#include "error.h"

// 在显卡的内存类型里，找一个既被 typeBits 允许、又满足 wanted 标志的。
//
// 显卡的内存不是一整块：有的只有 GPU 能碰（最快，但 CPU 写不进去），
// 有的 CPU 也能映射来直接写（方便，但 GPU 读起来慢一些），
// 独显和集显的划分方式还完全不同。Vulkan 不替你选，它把所有类型列出来让你挑。
//
// 两个输入的含义不同：
//   typeBits  驱动给出的限制，说明这个 buffer/image 只能放在哪几种内存上。
//             它是个位图，第 i 位为 1 表示第 i 种内存类型可用。
//             这个值来自 vkGetBufferMemoryRequirements。
//   wanted    调用方的要求，例如「CPU 得能映射」。
//
// 两边取交集，第一个满足的就是答案。
bool Application::findMemoryType(uint32_t typeBits, VkMemoryPropertyFlags wanted, uint32_t* out)
{
    VkPhysicalDeviceMemoryProperties properties{};
    vkGetPhysicalDeviceMemoryProperties(physicalDevice_, &properties);

    for (uint32_t i = 0; i < properties.memoryTypeCount; ++i) {
        // 这一种是不是在驱动允许的范围内
        const bool allowed = (typeBits & (1u << i)) != 0;
        // 这里是 == wanted 而不是 != 0，因为 wanted 里的每一位都必须满足。
        const bool matches = (properties.memoryTypes[i].propertyFlags & wanted) == wanted;
        if (allowed && matches) {
            *out = i;
            return true;
        }
    }

    reportError("找不到满足要求的显存类型（flags = 0x%x）", wanted);
    return false;
}
