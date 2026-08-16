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
    float color[3];     // 线性 RGB，会在三个顶点之间插值
};

// 三角形的三个顶点。改坐标或颜色，重新 vkx run 就能看到变化。
constexpr Vertex kVertices[] = {
    {{ 0.0f, -0.6f}, {1.0f, 0.0f, 0.0f}},   // 上，红
    {{ 0.6f,  0.6f}, {0.0f, 1.0f, 0.0f}},   // 右下，绿
    {{-0.6f,  0.6f}, {0.0f, 0.0f, 1.0f}},   // 左下，蓝
    // 想画更多三角形，在这里继续加顶点（每三个一组）。
};
