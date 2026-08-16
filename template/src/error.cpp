// error.h 的实现。
#include "error.h"

#include <cstdarg>
#include <cstdio>

#if defined(_WIN32)
#include <io.h>
#else
#include <unistd.h>
#endif

namespace {

// 标准错误是不是接在终端上。
bool stderrIsTerminal()
{
#if defined(_WIN32)
    return _isatty(_fileno(stderr)) != 0;
#else
    return isatty(fileno(stderr)) != 0;
#endif
}

}  // namespace

// 报告一条错误：写日志，必要时再弹窗。
//
// 弹窗只在没有终端可看的时候出现（双击运行、手机上）。有终端时不弹，
// 因为模态窗口在 CI 或脚本里没人点，会把进程一直挂住。
void reportError(const char* format, ...)
{
    char message[1024];
    va_list args;
    va_start(args, format);
    SDL_vsnprintf(message, sizeof(message), format, args);
    va_end(args);

    SDL_LogError(SDL_LOG_CATEGORY_APPLICATION, "%s", message);

    if (!stderrIsTerminal()) {
        SDL_ShowSimpleMessageBox(SDL_MESSAGEBOX_ERROR, "{{PROJECT_NAME}}", message, nullptr);
    }
}

// 把 VkResult 转成可读的名字，用于错误信息。
const char* vkResultName(VkResult result)
{
    switch (result) {
    case VK_SUCCESS:                        return "VK_SUCCESS";
    case VK_NOT_READY:                      return "VK_NOT_READY";
    case VK_TIMEOUT:                        return "VK_TIMEOUT";
    case VK_INCOMPLETE:                     return "VK_INCOMPLETE";
    case VK_SUBOPTIMAL_KHR:                 return "VK_SUBOPTIMAL_KHR";
    case VK_ERROR_OUT_OF_HOST_MEMORY:       return "VK_ERROR_OUT_OF_HOST_MEMORY";
    case VK_ERROR_OUT_OF_DEVICE_MEMORY:     return "VK_ERROR_OUT_OF_DEVICE_MEMORY";
    case VK_ERROR_INITIALIZATION_FAILED:    return "VK_ERROR_INITIALIZATION_FAILED";
    case VK_ERROR_DEVICE_LOST:              return "VK_ERROR_DEVICE_LOST";
    case VK_ERROR_LAYER_NOT_PRESENT:        return "VK_ERROR_LAYER_NOT_PRESENT";
    case VK_ERROR_EXTENSION_NOT_PRESENT:    return "VK_ERROR_EXTENSION_NOT_PRESENT";
    case VK_ERROR_FEATURE_NOT_PRESENT:      return "VK_ERROR_FEATURE_NOT_PRESENT";
    case VK_ERROR_INCOMPATIBLE_DRIVER:      return "VK_ERROR_INCOMPATIBLE_DRIVER";
    case VK_ERROR_SURFACE_LOST_KHR:         return "VK_ERROR_SURFACE_LOST_KHR";
    case VK_ERROR_OUT_OF_DATE_KHR:          return "VK_ERROR_OUT_OF_DATE_KHR";
    // 用到新的返回值时，在这里补上对应的名字。
    default:                                return "VK_ERROR_<未知>";
    }
}
