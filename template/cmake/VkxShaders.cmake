# 着色器的构建规则：.slang -> .spv -> .h。
#
# 用法：
#   vkx_add_slang_shader(<target>
#       SOURCE shaders/triangle.slang   # 着色器源文件
#       ENTRY  vertexMain               # 入口函数名
#       STAGE  vertex                   # 着色器阶段：vertex / fragment / compute ...
#       VAR    kTriangleVertSpv         # 生成的 C 数组变量名
#       [HEADER triangle_vert.spv.h])   # 生成的头文件名，默认是 <VAR>.h
#
# 生成的头文件里是一个 4 字节对齐的字节数组和它的长度，
# 可以直接交给 vkCreateShaderModule。

# 找 slangc。VKX_SLANGC 一般由 vkx 在配置时用 -DVKX_SLANGC=... 指定；
# 没指定就按下面的顺序找。
if(NOT VKX_SLANGC)
    find_program(VKX_SLANGC
        NAMES slangc
        HINTS
            "$ENV{VULKAN_SDK}/bin"
            "$ENV{VULKAN_SDK}/Bin"
            "$ENV{HOME}/.vkx/tools/slang/bin"
            "$ENV{USERPROFILE}/.vkx/tools/slang/bin"
        DOC "Slang 着色器编译器 (slangc)")
endif()

if(NOT VKX_SLANGC)
    message(FATAL_ERROR
        "找不到 slangc（Slang 着色器编译器）。\n"
        "  · 重新运行 vkx 的安装脚本，它会把 slangc 装到 ~/.vkx/tools/slang；或\n"
        "  · 安装 Vulkan SDK（1.3.296 起自带 slangc）并设置 VULKAN_SDK 环境变量；或\n"
        "  · 手动指定：cmake -DVKX_SLANGC=/path/to/slangc")
endif()

# 把 .spv 转成头文件的脚本，下面每条规则的第二步会调用它。
set(VKX_EMBED_SCRIPT "${CMAKE_CURRENT_LIST_DIR}/VkxEmbed.cmake")

function(vkx_add_slang_shader target)
    cmake_parse_arguments(ARG "" "SOURCE;ENTRY;STAGE;VAR;HEADER" "" ${ARGN})

    foreach(required SOURCE ENTRY STAGE VAR)
        if(NOT ARG_${required})
            message(FATAL_ERROR "vkx_add_slang_shader: 缺少参数 ${required}")
        endif()
    endforeach()

    if(NOT ARG_HEADER)
        set(ARG_HEADER "${ARG_VAR}.h")
    endif()

    get_filename_component(source_abs "${ARG_SOURCE}" ABSOLUTE
        BASE_DIR "${CMAKE_CURRENT_SOURCE_DIR}")

    # 中间产物 .spv 和最终的 .h 都放在构建目录里，不污染源码树。
    set(spv_dir "${CMAKE_BINARY_DIR}/vkx-shaders")
    set(gen_dir "${CMAKE_BINARY_DIR}/vkx-generated")
    set(spv_file "${spv_dir}/${ARG_VAR}.spv")
    set(header_file "${gen_dir}/${ARG_HEADER}")

    file(MAKE_DIRECTORY "${spv_dir}" "${gen_dir}")

    # 着色器之间用 #include 共享代码（比如 color.slang），把它们所在的目录交给
    # slangc 当搜索路径。同时把这些文件列进 DEPENDS，改了共享代码也会触发重编。
    get_filename_component(source_dir "${source_abs}" DIRECTORY)
    file(GLOB shader_includes "${source_dir}/*.slang")

    # 两步：slangc 编出 SPIR-V，再用脚本转成头文件。
    # DEPENDS 让 .slang 改动后自动重编。
    add_custom_command(
        OUTPUT "${header_file}"
        COMMAND "${VKX_SLANGC}" "${source_abs}"
                -target spirv
                -I "${source_dir}"
                -entry ${ARG_ENTRY}
                -stage ${ARG_STAGE}
                -o "${spv_file}"
        COMMAND "${CMAKE_COMMAND}"
                -DVKX_EMBED_INPUT=${spv_file}
                -DVKX_EMBED_OUTPUT=${header_file}
                -DVKX_EMBED_VAR=${ARG_VAR}
                -P "${VKX_EMBED_SCRIPT}"
        DEPENDS "${source_abs}" ${shader_includes} "${VKX_EMBED_SCRIPT}"
        BYPRODUCTS "${spv_file}"
        COMMENT "slangc ${ARG_SOURCE} [${ARG_ENTRY}] -> ${ARG_HEADER}"
        VERBATIM)

    # 挂一个中间目标，保证编译 .cpp 之前头文件已经生成好。
    set(stamp_target "${target}_shader_${ARG_VAR}")
    add_custom_target(${stamp_target} DEPENDS "${header_file}")
    add_dependencies(${target} ${stamp_target})

    # 让 target 能 #include 到生成目录里的头文件。
    target_sources(${target} PRIVATE "${header_file}")
    set_source_files_properties("${header_file}" PROPERTIES GENERATED TRUE HEADER_FILE_ONLY TRUE)
    target_include_directories(${target} PRIVATE "${gen_dir}")
endfunction()
