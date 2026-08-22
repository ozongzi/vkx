package {{PACKAGE_ID}};

import org.libsdl.app.SDLActivity;

/**
 * App 在 Android 上的入口 Activity。
 *
 * 继承自 SDL 提供的 SDLActivity，由它负责创建窗口、转发触摸和生命周期事件，
 * 最后加载下面列出的原生库并调用 main.cpp 里的 main()。
 */
public class MainActivity extends SDLActivity {
    /** 启动时按顺序加载的原生库。libmain.so 依赖 libSDL3.so，所以 SDL3 在前。 */
    @Override
    protected String[] getLibraries() {
        return new String[] {
            "SDL3",
            "main",
        };
    }

    // 需要调用 Android API（震动、账号、内购……）时，在这里加方法，
    // 用 JNI 从 C++ 侧调过来。
}
