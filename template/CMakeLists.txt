# {{PROJECT_NAME}} 的构建脚本，五个平台共用这一份。
#
# 从上到下：编译选项 -> 拉依赖 -> 定义目标 -> 各平台的链接 -> 着色器编译。
#
# 平台由 vkx 通过命令行告诉 CMake，脚本里用这两个变量分支：
#   ANDROID  为真时在交叉编译 Android
#   IOS      为真时在交叉编译 iOS

cmake_minimum_required(VERSION 3.24)

project({{PROJECT_NAME}} LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_C_STANDARD 11)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Debug)
endif()

# cmake/ 下放的是本工程自己的 CMake 模块，VkxShaders 提供 vkx_add_slang_shader()。
list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(FetchContent)
include(VkxShaders)

# 依赖的版本都钉死在这里，改这几行就能升级。
set(VKX_SDL_TAG "release-3.4.14" CACHE STRING "SDL3 git tag used when SDL3 is fetched from source")
set(VKX_VULKAN_HEADERS_TAG "v1.4.313" CACHE STRING "Vulkan-Headers git tag")
set(VKX_VOLK_TAG "1.4.304" CACHE STRING "volk git tag")

# ---------------------------------------------------------------------------
# 依赖
# ---------------------------------------------------------------------------

# 从 GitHub 拉 SDL3 源码一起编译。
function(vkx_fetch_sdl)
    message(STATUS "vkx: 从源码构建 SDL3 (${VKX_SDL_TAG})")
    set(SDL_SHARED OFF CACHE BOOL "" FORCE)
    set(SDL_STATIC ON CACHE BOOL "" FORCE)
    set(SDL_TEST_LIBRARY OFF CACHE BOOL "" FORCE)
    set(SDL_EXAMPLES OFF CACHE BOOL "" FORCE)
    set(SDL_INSTALL OFF CACHE BOOL "" FORCE)
    FetchContent_Declare(SDL3
        GIT_REPOSITORY https://github.com/libsdl-org/SDL.git
        GIT_TAG ${VKX_SDL_TAG}
        GIT_SHALLOW TRUE)
    FetchContent_MakeAvailable(SDL3)
endfunction()

# SDL3 按平台分三种来源：
if(ANDROID)
    # Gradle 已经通过 prefab 把官方 .aar 里的 SDL3 准备好了。
    find_package(SDL3 REQUIRED CONFIG)
elseif(IOS OR FETCHCONTENT_SOURCE_DIR_SDL3)
    # iOS：系统里装的 SDL3 是 macOS 版，用不了，只能交叉编译。
    # 另一种情况是 vkx 用 FETCHCONTENT_SOURCE_DIR_SDL3 指定了本地源码，
    # 这时也一律从源码静态编译，产物不依赖机器上装了什么。
    vkx_fetch_sdl()
else()
    # 没有本地源码时退而求其次：有系统版就用，没有就联网拉。
    find_package(SDL3 3.2 QUIET CONFIG)
    if(NOT SDL3_FOUND)
        vkx_fetch_sdl()
    endif()
endif()

# Vulkan 头文件。拉固定版本而不是用系统里的，保证各平台编出来的一致。
FetchContent_Declare(VulkanHeaders
    GIT_REPOSITORY https://github.com/KhronosGroup/Vulkan-Headers.git
    GIT_TAG ${VKX_VULKAN_HEADERS_TAG}
    GIT_SHALLOW TRUE)
FetchContent_MakeAvailable(VulkanHeaders)

# volk：运行期把 Vulkan 函数指针加载出来，链接期就不必依赖 Vulkan loader。
# iOS 不需要它（那边静态链接 MoltenVK，函数本来就在二进制里）。
if(NOT IOS)
    FetchContent_Declare(volk
        GIT_REPOSITORY https://github.com/zeux/volk.git
        GIT_TAG ${VKX_VOLK_TAG}
        GIT_SHALLOW TRUE)
    # VOLK_PULL_IN_VULKAN 关掉 volk 自己找头文件的逻辑，
    # 改由下面这行把上面拉的 Vulkan-Headers 直接接给它。
    # （交叉编译时 volk 的自动探测会摸到宿主机的头文件。）
    set(VOLK_PULL_IN_VULKAN OFF CACHE BOOL "" FORCE)
    FetchContent_MakeAvailable(volk)
    target_link_libraries(volk PUBLIC Vulkan::Headers)
endif()

# ---------------------------------------------------------------------------
# 可执行目标
# ---------------------------------------------------------------------------

# 新增 .cpp 文件时加到这里。每个文件对应流程里的一步，顺序和 init() 一致。
set(VKX_SOURCES
    src/main.cpp            # 入口、初始化顺序、主循环
    src/error.cpp           # 错误上报
    src/instance.cpp        # 实例、校验层、窗口表面
    src/device.cpp          # 挑显卡、逻辑设备
    src/swapchain.cpp       # 交换链
    src/vertex_buffer.cpp   # 顶点缓冲
    src/pipeline.cpp        # 着色器模块、图形管线
    src/frame.cpp)          # 每帧资源、录制命令、画一帧

if(ANDROID)
    # Android 上没有可执行文件：Java 层的 SDLActivity 会去加载名为 main 的共享库，
    # 名字必须是 main，SDL 找的就是 libmain.so。
    add_library(main SHARED ${VKX_SOURCES})
    set(VKX_TARGET main)
else()
    add_executable({{PROJECT_NAME}} ${VKX_SOURCES})
    set(VKX_TARGET {{PROJECT_NAME}})
endif()

target_link_libraries(${VKX_TARGET} PRIVATE SDL3::SDL3)
# VKX_DEBUG 只在 Debug 配置下定义，main.cpp 用它开关校验层。
target_compile_definitions(${VKX_TARGET} PRIVATE $<$<CONFIG:Debug>:VKX_DEBUG=1>)

# 需要别的库（数学库、网络库……）时，在这里 target_link_libraries。

# Vulkan 函数的两种接法：
if(IOS)
    # 静态链接的 MoltenVK 已提供全部函数，用头文件里的原型直接调用。
    target_link_libraries(${VKX_TARGET} PRIVATE Vulkan::Headers)
    target_compile_definitions(${VKX_TARGET} PRIVATE VKX_STATIC_VULKAN=1)
else()
    # VK_NO_PROTOTYPES 去掉头文件里的函数声明，改由 volk 提供同名函数指针。
    target_link_libraries(${VKX_TARGET} PRIVATE volk)
    target_compile_definitions(${VKX_TARGET} PRIVATE VK_NO_PROTOTYPES)
endif()

# macOS：把可执行文件放到构建目录根上，vkx run 按这个位置找它。
if(APPLE AND NOT IOS)
    set_target_properties(${VKX_TARGET} PROPERTIES
        RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
endif()

if(IOS)
    # iOS 的产物是一个 .app 包，这些属性描述包怎么生成。
    set_target_properties(${VKX_TARGET} PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/ios/Info.plist"
        XCODE_ATTRIBUTE_PRODUCT_BUNDLE_IDENTIFIER "{{PACKAGE_ID}}"
        XCODE_ATTRIBUTE_TARGETED_DEVICE_FAMILY "1,2")   # 1=iPhone 2=iPad

    if(NOT VKX_MOLTENVK_LIB)
        message(FATAL_ERROR
            "iOS 构建需要 -DVKX_MOLTENVK_LIB=<libMoltenVK.a 的路径>。\n"
            "正常情况下 `vkx build --target ios` 会自动下载并传入。")
    endif()

    # iOS 上没有 Vulkan loader，Vulkan 由静态库 MoltenVK 提供，
    # 它把 Vulkan 调用翻译成 Metal，因此要一并链接下面这几个系统框架。
    #
    # -force_load 会把静态库里的全部目标文件都拉进来。这里必须加：
    # 代码中没有一处直接引用 vkGetInstanceProcAddr（SDL 是运行期用
    # dlsym 找它的），否则链接器会认为 MoltenVK 无人使用而整个丢弃。
    target_link_libraries(${VKX_TARGET} PRIVATE
        "-force_load ${VKX_MOLTENVK_LIB}"
        "-framework Metal"
        "-framework Foundation"
        "-framework QuartzCore"
        "-framework CoreGraphics"
        "-framework IOSurface"
        "-framework UIKit")
endif()

# ---------------------------------------------------------------------------
# 着色器：Slang -> SPIR-V -> 嵌入头文件
# ---------------------------------------------------------------------------
# 每条规则处理一个入口点，生成的头文件由 src/pipeline.cpp 直接 #include。
# 加新着色器时在下面照着补一条。

vkx_add_slang_shader(${VKX_TARGET}
    SOURCE shaders/triangle.slang
    ENTRY  vertexMain
    STAGE  vertex
    VAR    kTriangleVertSpv
    HEADER triangle_vert.spv.h)

vkx_add_slang_shader(${VKX_TARGET}
    SOURCE shaders/triangle.slang
    ENTRY  fragmentMain
    STAGE  fragment
    VAR    kTriangleFragSpv
    HEADER triangle_frag.spv.h)
