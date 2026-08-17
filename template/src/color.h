// 输出色域，以及把颜色转到输出色域的那点数学。
//
// 为什么需要这个文件：
//
// 现在的显示器很多是广色域的（Apple 的屏幕、大部分新显示器都是 Display P3），
// 能显示的颜色比 sRGB 多出一圈——最明显的是青绿一带，sRGB 里那点青色一直很憋屈。
// Vulkan 通过 VK_EXT_swapchain_colorspace 允许我们直接输出到 P3，把这块用上。
//
// 代价是必须把颜色转过去。同一组 RGB 数值在 sRGB 和 P3 两套基色下是不同的颜色：
// sRGB 的纯红 (1, 0, 0) 在 P3 里是 (0.822, 0.033, 0.017)。把交换链切到 P3 却继续
// 送原来的数值，等于把每个颜色都往外推了一截，画面会整体过饱和——这是广色域最
// 常见的翻车方式。所以这里提供一个转换函数，写颜色时照旧按 sRGB 想，输出前过一道。
#pragma once

namespace vkx {

// 交换链最终输出到哪个色域。由 createSwapchain() 按显示器实际支持的情况选定。
enum class Gamut {
    Srgb,       // 传统 sRGB，所有平台都有
    DisplayP3,  // 广色域，比 sRGB 大一圈
};

// 把一个线性 sRGB 颜色转到目标色域，原地改。
//
// 传 Gamut::Srgb 时什么都不做，所以调用处不需要分支。
//
// 注意输入必须是线性值，不是你在取色器里看到的那个 sRGB 数值——两者差一条 gamma
// 曲线。本工程的颜色一律按线性写（原因见 swapchain.cpp 里挑格式那段）。
void toGamut(Gamut gamut, float rgb[3]);

// 色域的名字，只用来打日志。
const char* gamutName(Gamut gamut);

}  // namespace vkx
