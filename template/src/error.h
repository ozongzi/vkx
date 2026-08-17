// 出错时怎么报。
//
// 每个可能失败的函数都返回 bool，用 VKX_CHECK 把 Vulkan 的返回值转成这个约定：
// 不是 VK_SUCCESS 就报错并让当前函数返回 false，一路短路到 main。
#pragma once

#include "vulkan_api.h"

// 把一条错误写进日志；没有终端可看时（双击运行、手机上）额外弹个窗。
void report_error(const char* format, ...);

// 把 VkResult 转成可读的名字，用于错误信息。
const char* vk_result_name(VkResult result);

// 执行一句 Vulkan 调用；不是 VK_SUCCESS 就报错并让当前函数返回 false。
// 报错信息里带上调用的原文、返回值名字和出错行号。
#define VKX_CHECK(expr)                                                        \
    do {                                                                       \
        VkResult vkx_result = (expr);                                           \
        if (vkx_result != VK_SUCCESS) {                                         \
            report_error("%s\n  返回 %s\n  位置 %s:%d",                          \
                        #expr, vk_result_name(vkx_result), __FILE__, __LINE__);   \
            return false;                                                      \
        }                                                                      \
    } while (false)
