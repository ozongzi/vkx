// 三角形的顶点数据。
//
// 想改三角形的形状或颜色，改这个文件就够了。顶点布局改动时，
// pipeline.cpp 里的属性描述和 shaders/triangle.slang 里的 VertexInput 要跟着改。
#pragma once

#include <cstddef>

// 一个顶点的内存布局。改这里要同步改 createPipeline() 里的属性描述，
// 以及 shaders/triangle.slang 里的 VertexInput。
struct Vertex {
    float position[2];  // 裁剪空间坐标，范围 [-1, 1]，Y 轴向下
    // 线性 RGB，会在三个顶点之间插值。
    //
    // 数值是相对交换链输出色域的（见 color.h）。在 Display P3 的屏幕上，
    // 下面那三个 1.0 拿到的就是 P3 的基色，比 sRGB 的红绿蓝更鲜。
    float color[3];
};

// 三角形的三个顶点。改坐标或颜色，重新 vkx run 就能看到变化。
constexpr Vertex kVertices[] = {
    {{ 0.0f, -0.6f}, {1.0f, 0.0f, 0.0f}},   // 上，红（输出色域的纯红）
    {{ 0.6f,  0.6f}, {0.0f, 1.0f, 0.0f}},   // 右下，绿
    {{-0.6f,  0.6f}, {0.0f, 0.0f, 1.0f}},   // 左下，蓝
    // 想画更多三角形，在这里继续加顶点（每三个一组）。
};
