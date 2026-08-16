// 第四步：把顶点数据搬进显存。
#include "app.h"
#include "error.h"
#include "vertex.h"

// 在显卡的内存类型里，找一个既被 typeBits 允许、又满足 wanted 标志的。
bool Application::findMemoryType(uint32_t typeBits, VkMemoryPropertyFlags wanted, uint32_t* out)
{
    VkPhysicalDeviceMemoryProperties properties{};
    vkGetPhysicalDeviceMemoryProperties(physicalDevice_, &properties);

    for (uint32_t i = 0; i < properties.memoryTypeCount; ++i) {
        const bool allowed = (typeBits & (1u << i)) != 0;
        const bool matches = (properties.memoryTypes[i].propertyFlags & wanted) == wanted;
        if (allowed && matches) {
            *out = i;
            return true;
        }
    }

    reportError("找不到满足要求的显存类型（flags = 0x%x）", wanted);
    return false;
}

// 建一个顶点缓冲，把 kVertices 拷进去。
// 三步走：创建 buffer（只是个描述）-> 分配显存 -> 绑定并写入。
bool Application::createVertexBuffer()
{
    const VkDeviceSize size = sizeof(kVertices);

    VkBufferCreateInfo bufferInfo{};
    bufferInfo.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO;
    bufferInfo.size = size;
    bufferInfo.usage = VK_BUFFER_USAGE_VERTEX_BUFFER_BIT;
    bufferInfo.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
    VKX_CHECK(vkCreateBuffer(device_, &bufferInfo, nullptr, &vertexBuffer_));

    // 驱动给出这个 buffer 需要多大、可以放在哪些内存类型上。
    VkMemoryRequirements requirements{};
    vkGetBufferMemoryRequirements(device_, vertexBuffer_, &requirements);

    // HOST_VISIBLE：CPU 能映射来写；HOST_COHERENT：写完不用手动 flush。
    // 数据量大起来之后，一般改成 device-local 显存 + 暂存缓冲上传。
    uint32_t memoryType = 0;
    if (!findMemoryType(requirements.memoryTypeBits,
                        VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                        &memoryType)) {
        return false;
    }

    VkMemoryAllocateInfo allocInfo{};
    allocInfo.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
    allocInfo.allocationSize = requirements.size;
    allocInfo.memoryTypeIndex = memoryType;
    VKX_CHECK(vkAllocateMemory(device_, &allocInfo, nullptr, &vertexMemory_));
    VKX_CHECK(vkBindBufferMemory(device_, vertexBuffer_, vertexMemory_, 0));

    // 映射成 CPU 指针，memcpy 进去，再解除映射。
    void* mapped = nullptr;
    VKX_CHECK(vkMapMemory(device_, vertexMemory_, 0, size, 0, &mapped));
    SDL_memcpy(mapped, kVertices, static_cast<size_t>(size));
    vkUnmapMemory(device_, vertexMemory_);
    return true;
}
