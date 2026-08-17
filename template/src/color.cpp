// color.h 的实现。
#include "color.h"

namespace vkx {

const char* gamut_name(Gamut gamut)
{
    switch (gamut) {
    case Gamut::DisplayP3: return "Display P3";
    case Gamut::Srgb:      return "sRGB";
    }
    return "未知";
}

}  // namespace vkx
