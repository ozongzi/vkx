# 把一个 .spv 文件转成可以直接 #include 的 C++ 头文件。
# 以脚本模式运行（cmake -P），由 VkxShaders.cmake 里的规则调用：
#
#   cmake -DVKX_EMBED_INPUT=a.spv -DVKX_EMBED_OUTPUT=a.h -DVKX_EMBED_VAR=kA -P VkxEmbed.cmake
#
# 产出形如：
#   alignas(uint32_t) static const unsigned char kA[] = { 0x03,0x02,... };
#   static const size_t kA_size = 1152;

foreach(required VKX_EMBED_INPUT VKX_EMBED_OUTPUT VKX_EMBED_VAR)
    if(NOT ${required})
        message(FATAL_ERROR "VkxEmbed.cmake: 缺少 -D${required}=...")
    endif()
endforeach()

if(NOT EXISTS "${VKX_EMBED_INPUT}")
    message(FATAL_ERROR "VkxEmbed.cmake: 输入文件不存在: ${VKX_EMBED_INPUT}")
endif()

# 以十六进制字符串读入整个文件，两个字符对应一个字节。
file(READ "${VKX_EMBED_INPUT}" hex HEX)
string(LENGTH "${hex}" hex_length)
math(EXPR byte_count "${hex_length} / 2")

if(byte_count EQUAL 0)
    message(FATAL_ERROR "VkxEmbed.cmake: ${VKX_EMBED_INPUT} 是空文件，着色器编译多半失败了")
endif()

# SPIR-V 是 32 位字的序列，长度必然是 4 的倍数。
math(EXPR remainder "${byte_count} % 4")
if(NOT remainder EQUAL 0)
    message(FATAL_ERROR "VkxEmbed.cmake: SPIR-V 长度 ${byte_count} 不是 4 的倍数，文件已损坏")
endif()

# 拆成一个个字节，拼成 0xNN, 的形式，每 12 个换一行。
string(REGEX MATCHALL "[0-9a-f][0-9a-f]" bytes "${hex}")

set(body "    ")
set(column 0)
foreach(byte IN LISTS bytes)
    string(APPEND body "0x${byte},")
    math(EXPR column "${column} + 1")
    if(column EQUAL 12)
        string(APPEND body "\n    ")
        set(column 0)
    endif()
endforeach()

get_filename_component(source_name "${VKX_EMBED_INPUT}" NAME)

# alignas(uint32_t) 是必需的：vkCreateShaderModule 要求 pCode 按 4 字节对齐。
set(content "// 由 vkx 生成，请勿手动修改。源: ${source_name}\n")
string(APPEND content "#pragma once\n\n")
string(APPEND content "#include <cstddef>\n#include <cstdint>\n\n")
string(APPEND content "alignas(uint32_t) static const unsigned char ${VKX_EMBED_VAR}[] = {\n")
string(APPEND content "${body}\n};\n\n")
string(APPEND content "static const size_t ${VKX_EMBED_VAR}_size = ${byte_count};\n")

# 内容没变就不要重写，免得触发无谓的重新编译。
set(previous "")
if(EXISTS "${VKX_EMBED_OUTPUT}")
    file(READ "${VKX_EMBED_OUTPUT}" previous)
endif()
if(NOT previous STREQUAL content)
    file(WRITE "${VKX_EMBED_OUTPUT}" "${content}")
endif()
