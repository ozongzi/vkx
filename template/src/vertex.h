// 三角形的顶点数据。
//
// 想改三角形的形状或颜色，改这个文件就够了。顶点布局改动时，
// pipeline.cpp 里的属性描述和 shaders/triangle.slang 里的 VertexInput 要跟着改。
#pragma once

#include <cstddef>

// 一个顶点的内存布局。改这里要同步改 create_pipeline() 里的属性描述，
// 以及 shaders/triangle.slang 里的 VertexInput。
struct Vertex {
    float position[2];  // 裁剪空间坐标，范围 [-1, 1]，Y 轴向下
    // 颜色，OKLCH：亮度 0~1、彩度 0~0.4、色相 0~360 度。
    //
    // 本工程的颜色一律写成 OKLCH，不写 RGB。原因见 shaders/color.slang 开头：
    // OKLCH 是绝对的（同一个值在任何屏幕上是同一个颜色）而且感知均匀。
    // 转成 RGB 由着色器负责，超出屏幕能力的颜色会自动映射回来。
    float oklch[3];
};

// 三角形的三个顶点。改坐标或颜色，重新 vkx run 就能看到变化。
// 三个顶点用同样的亮度和彩度，只把色相拉开 120 度——于是拿到的是三个鲜艳程度
// 和明暗观感完全一致的颜色。这件事在 RGB 里做不到，是 OKLCH 最直接的好处。
//
// 彩度给到 0.30 是刻意的：它超出了 sRGB 在某些色相上能表达的范围。在广色域屏幕
// 上你会看到更鲜的颜色，在 sRGB 屏幕上则被自动映射到该色相最鲜的位置。
constexpr Vertex VERTICES[] = {
    {{ 0.0f, -0.6f}, {0.65f, 0.30f,  25.0f}},   // 上，红
    {{ 0.6f,  0.6f}, {0.65f, 0.30f, 145.0f}},   // 右下，绿
    {{-0.6f,  0.6f}, {0.65f, 0.30f, 265.0f}},   // 左下，蓝
    // 想画更多三角形，在这里继续加顶点（每三个一组）。
};
