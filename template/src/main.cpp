// {{PROJECT_NAME}} —— Vulkan + SDL3，在窗口里画一个三角形。
//
// 整个程序的入口，只有一个 main()。想知道各步骤在哪，见 app.h 顶部的文件清单。
#include "app.h"

#include <SDL3/SDL_main.h>

// 程序入口。
//
// SDL_main.h 在 Windows、Android、iOS 上会把这个 main 接到各平台真正的入口上
// （Windows 是 WinMain，Android 是 Java 层调过来的，iOS 是 UIApplicationMain），
// 所以这里只写标准 main 就够了。
//
// 这里没有清理代码。app 是栈上对象，无论从哪条路径返回，~Application()
// 都会执行，把已经建好的资源释放掉。
int main(int argc, char* argv[])
{
    (void)argc;
    (void)argv;

    Application app;
    if (!app.init()) {
        return 1;   // 建到一半失败，析构函数会把已经建好的那部分拆掉
    }
    app.run();
    return 0;
}
