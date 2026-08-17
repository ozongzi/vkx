// 第三步：校验层的消息出口。
//
// 校验层本身在 instance.cpp 里已经挂上了，但它只是「检查」，
// 检查出来的问题要送到哪儿去，得由这里注册一个回调告诉它。
// 没有这一步，校验层查出的错误会被默默丢掉。
//
// 整个文件在 Release 构建里是空的：函数还在（init() 的调用链要用），
// 但函数体被 #if 掉，直接返回 true。
#include "app.h"
#include "error.h"

#if VKX_DEBUG
namespace {

// 校验层的消息出口。Debug 构建里，用错 Vulkan API 的信息会从这里打印出来。
//
// 返回值 VK_FALSE 表示这条消息已经处理完，引发它的那次 Vulkan 调用照常继续。
// 返回 VK_TRUE 会让那次调用直接失败，那是校验层自测用的，应用程序一律返回
// VK_FALSE。
VKAPI_ATTR VkBool32 VKAPI_CALL debugCallback(VkDebugUtilsMessageSeverityFlagBitsEXT severity,
                                             VkDebugUtilsMessageTypeFlagsEXT,
                                             const VkDebugUtilsMessengerCallbackDataEXT* data,
                                             void*)
{
    // 严重程度是有序的枚举，所以可以直接比大小：只打印警告及以上。
    // 想看更啰嗦的信息（每次创建对象都会有一条），把这个条件放宽，
    // 同时也要把下面 messageSeverity 里的 INFO / VERBOSE 位打开。
    if (severity >= VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT) {
        SDL_LogWarn(SDL_LOG_CATEGORY_GPU, "[validation] %s", data->pMessage);
    }
    return VK_FALSE;
}

}  // namespace
#endif

// 把上面那个 debugCallback 注册给校验层。
bool Application::createDebugMessenger()
{
#if VKX_DEBUG
    // 实例创建时没挂上校验层（比如没装 Vulkan SDK），这里就没什么可注册的。
    // 强行调用 vkCreateDebugUtilsMessengerEXT 会因为扩展没启用而崩。
    if (!validationEnabled_) {
        return true;
    }

    VkDebugUtilsMessengerCreateInfoEXT messengerInfo{};
    messengerInfo.sType = VK_STRUCTURE_TYPE_DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT;
    // 想收哪些严重程度的消息。
    messengerInfo.messageSeverity = VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT
                                  | VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT;
    // 想收哪几类消息：一般信息、规范违规、性能建议。
    messengerInfo.messageType = VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT
                              | VK_DEBUG_UTILS_MESSAGE_TYPE_VALIDATION_BIT_EXT
                              | VK_DEBUG_UTILS_MESSAGE_TYPE_PERFORMANCE_BIT_EXT;
    messengerInfo.pfnUserCallback = debugCallback;

    // 这里故意不用 VKX_CHECK：messenger 只是个调试辅助，建不出来
    // 也不该让程序起不来。失败的话 debugMessenger_ 保持 VK_NULL_HANDLE，
    // 析构时的判空会跳过它。
    vkCreateDebugUtilsMessengerEXT(instance_, &messengerInfo, nullptr, &debugMessenger_);
#endif
    return true;
}
