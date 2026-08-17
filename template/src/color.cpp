// color.h 的实现。
#include "color.h"

namespace vkx {
namespace {

// 线性 sRGB -> 线性 Display P3。
//
// 这个矩阵是这么来的：两个色域各自的 RGB -> XYZ 矩阵由它们的基色坐标和白点算出
// （sRGB 基色 R(0.640,0.330) G(0.300,0.600) B(0.150,0.060)，P3 是
// R(0.680,0.320) G(0.265,0.690) B(0.150,0.060)，白点都是 D65），
// 然后 sRGB->XYZ 再接 XYZ->P3，两个矩阵乘起来就是它。
//
// 两个可以自查的性质：
//   每一行加起来都是 1，所以白色 (1,1,1) 严格映射到白色，不会偏色；
//   sRGB 的三个基色映射进来之后都落在 [0,1] 内，因为 sRGB 完全被 P3 包住。
constexpr float kSrgbToDisplayP3[3][3] = {
    {+0.82246197f, +0.17753803f, +0.00000000f},
    {+0.03319420f, +0.96680580f, +0.00000000f},
    {+0.01708263f, +0.07239744f, +0.91051993f},
};

}  // namespace

void toGamut(Gamut gamut, float rgb[3])
{
    if (gamut == Gamut::Srgb) {
        return;   // 目标就是 sRGB，原样输出
    }

    const float r = rgb[0];
    const float g = rgb[1];
    const float b = rgb[2];
    for (int i = 0; i < 3; ++i) {
        rgb[i] = kSrgbToDisplayP3[i][0] * r
               + kSrgbToDisplayP3[i][1] * g
               + kSrgbToDisplayP3[i][2] * b;
    }
}

const char* gamutName(Gamut gamut)
{
    switch (gamut) {
    case Gamut::DisplayP3: return "Display P3";
    case Gamut::Srgb:      return "sRGB";
    }
    return "未知";
}

}  // namespace vkx
