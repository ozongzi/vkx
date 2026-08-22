// 三角形的顶点着色器和片元着色器。
//
// 构建时 slangc 会把这个文件编译两次（各取一个入口），产出两段 SPIR-V，
// 再由 CMake 转成头文件嵌进可执行文件。规则见 cmake/VkxShaders.cmake。

#include "color.slang"

// 顶点着色器的输入，逐顶点从顶点缓冲里读。
// location 要和 pipeline.cpp 里的属性描述一一对应。
struct VertexInput {
    [[vk::location(0)]] float2 position;   // 对应 Vertex::position
    [[vk::location(1)]] float3 oklch;      // 对应 Vertex::oklch
    // 加新属性（UV、法线……）时，这里和 pipeline.cpp 两边都要加。
};

// 顶点着色器传给片元着色器的数据。
// SV_Position 是必须输出的裁剪空间坐标；其余字段会在三角形内部插值。
struct VertexOutput {
    float4 position : SV_Position;
    float3 color    : COLOR0;
};

// 顶点着色器：每个顶点跑一次。
//
// 坐标补成四维；颜色从 OKLCH 转成输出色域的 linear RGB（见 color.slang）。
// 转换放在这里而不是片元着色器：颜色是逐顶点的，这里一个顶点只算一次，
// 放到片元着色器就要每个像素算一次。
[shader("vertex")]
VertexOutput vertex_main(VertexInput input)
{
    VertexOutput output;
    output.position = float4(input.position, 0.0, 1.0);
    output.color    = oklch_to_linear_rgb(input.oklch);
    return output;
}

// 片元着色器：每个被覆盖的像素跑一次。
// input.color 是三个顶点颜色按重心坐标插值后的结果，所以三角形是渐变的。
//
// 注意插值发生在 linear RGB 上，不是 OKLCH 上。做小范围渐变没问题；真要让渐变
// 在感知上均匀（比如蓝到黄不经过灰），得把 OKLCH 传下来在这里逐像素转。
[shader("fragment")]
float4 fragment_main(VertexOutput input) : SV_Target
{
    return float4(input.color, 1.0);
}

// 再加别的着色器：在这个文件里写新的入口函数，或者新建一个 .slang 文件。
// 带 [shader("...")] 标注的入口 vkx 会自动扫出来，不用改任何配置。
