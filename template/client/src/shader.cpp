// 通用零件：把一段 SPIR-V 字节码包成 VkShaderModule。
//
// 和 memory.cpp 一样，这不是流程里的一步，而是每建一条管线都要用的工具。
#include "app.h"
#include "error.h"

// 字节码的来源：构建时 slangc 把 shaders/*.slang 编译成 SPIR-V（一种二进制
// 中间格式，显卡驱动认它，不认 GLSL/HLSL 源码），再由 CMake 的脚本转成一个
// C 数组塞进头文件，最后 #include 进可执行文件。规则见 cmake/VkxShaders.cmake。
//
// 着色器因此是编译进程序里的，运行时不需要额外的文件，打包分发时也不必
// 单独处理 .spv。
//
// VkShaderModule 只是字节码的一层薄包装，本身不做任何编译。
// 真正的编译发生在 vkCreateGraphicsPipelines：那时驱动才知道
// 完整的固定功能状态，能一起优化。所以管线一建好，模块就可以立刻销毁了。
bool Application::create_shader_module(const unsigned char* code, size_t size, VkShaderModule* out)
{
    VkShaderModuleCreateInfo info{};
    info.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO;
    info.codeSize = size;  // 单位是字节，不是 uint32 的个数
    // pCode 要求按 4 字节对齐（SPIR-V 是 32 位字的序列）。
    // 生成的头文件里那个数组带了 alignas(uint32_t)，所以这个 cast 是安全的。
    info.pCode = reinterpret_cast<const uint32_t*>(code);
    VKX_CHECK(vkCreateShaderModule(device, &info, nullptr, out));
    return true;
}
