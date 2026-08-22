// 第七步：把顶点数据搬进显存。
//
// 三角形的顶点现在还只是 CPU 内存里的一个数组（见 vertex.h），
// GPU 读不到。这一步把它变成一个 GPU 能访问的缓冲。
#include "app.h"
#include "error.h"
#include "vertex.h"

// 建一个顶点缓冲，把 VERTICES 拷进去。
//
// 三步走，这也是 Vulkan 里分配任何显存的固定套路：
//   1. 创建 VkBuffer —— 只是一个「描述」，说明要多大、拿来干什么，还没有内存
//   2. 分配 VkDeviceMemory —— 真正的一块显存
//   3. 绑定 + 写入 —— 把两者关联起来，再把数据拷进去
//
// 拆成两步而不是一次 malloc，是因为显存分配开销很大，驱动对同时存在的
// 分配数量也有上限。真实项目里会一次分配一大块，再自行切成小段分给不同的
// buffer，VMA 一类的库做的就是这件事。
bool Application::create_vertex_buffer()
{
    const VkDeviceSize size = sizeof(VERTICES);

    VkBufferCreateInfo buffer_info{};
    buffer_info.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO;
    buffer_info.size = size;
    // usage 告诉驱动这块缓冲拿来干什么，驱动可能据此选择不同的内部布局。
    // 想再拿它当索引缓冲，就按位或上 VK_BUFFER_USAGE_INDEX_BUFFER_BIT。
    buffer_info.usage = VK_BUFFER_USAGE_VERTEX_BUFFER_BIT;
    buffer_info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;  // 只有一个队列族会用它
    VKX_CHECK(vkCreateBuffer(device, &buffer_info, nullptr, &vertex_buffer));

    // 驱动给出这个 buffer 实际需要多大（可能比 size 大，有对齐要求）、
    // 以及可以放在哪些内存类型上。
    VkMemoryRequirements requirements{};
    vkGetBufferMemoryRequirements(device, vertex_buffer, &requirements);

    // HOST_VISIBLE：CPU 能映射来写；
    // HOST_COHERENT：CPU 写完 GPU 立刻可见，不用手动 vkFlushMappedMemoryRanges。
    //
    // 这是最省事的组合，但这类内存通常不是显卡本地的最快内存。
    // 数据量大起来之后，一般改成 device-local 显存 + 一个临时的「暂存缓冲」
    // 上传：先写进 host-visible 的暂存缓冲，再用 vkCmdCopyBuffer 拷到显卡本地。
    uint32_t memory_type = 0;
    if (!find_memory_type(
            requirements.memoryTypeBits,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            &memory_type)) {
        return false;
    }

    VkMemoryAllocateInfo alloc_info{};
    alloc_info.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
    alloc_info.allocationSize = requirements.size;  // 用驱动给出的大小，而不是 sizeof
    alloc_info.memoryTypeIndex = memory_type;
    VKX_CHECK(vkAllocateMemory(device, &alloc_info, nullptr, &vertex_memory));
    VKX_CHECK(vkBindBufferMemory(device, vertex_buffer, vertex_memory, 0));

    // 映射成一个普通的 CPU 指针，memcpy 进去，再解除映射。
    // 这些顶点建好之后就不会再变，所以映射完就可以撤掉；
    // 每帧都要改的数据（比如 UI）才需要一直映射着。
    void* mapped = nullptr;
    VKX_CHECK(vkMapMemory(device, vertex_memory, 0, size, 0, &mapped));
    SDL_memcpy(mapped, VERTICES, static_cast<size_t>(size));
    vkUnmapMemory(device, vertex_memory);
    return true;
}
