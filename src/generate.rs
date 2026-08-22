//! 生成 `target/CMakeLists.txt`。
//!
//! 工程里唯一要人编辑的是 `vkx.toml`。CMake 由这里从它生成，所以不同工程之间
//! 不会因为手改而分叉——教程里那些带行号的 diff 也就一直对得上。
//!
//! 生成物是给人读的：出问题时用户看到的是他没写过的文件，所以注释和排版都按
//! 手写的标准来。

use std::collections::BTreeSet;
use std::path::Path;

use crate::error::{Code, Error, Result};
use crate::project::Project;

/// 着色器里的一个入口点。
struct Entry {
    /// 相对工程根的路径，如 `shaders/triangle.slang`
    source: String,
    /// 函数名，如 `vertex_main`
    name: String,
    /// slang 的阶段名，如 `vertex`
    stage: String,
}

impl Entry {
    /// 阶段的三字母缩写，用来拼头文件名。
    fn short(&self) -> &str {
        match self.stage.as_str() {
            "vertex" => "vert",
            "fragment" | "pixel" => "frag",
            "compute" => "comp",
            "geometry" => "geom",
            other => other,
        }
    }

    /// `shaders/triangle.slang` + vertex → `triangle_vert.spv.h`
    fn header(&self) -> String {
        format!("{}_{}.spv.h", self.stem(), self.short())
    }

    /// 同上 → `TRIANGLE_VERT_SPV`
    fn var(&self) -> String {
        format!("{}_{}_SPV", self.stem(), self.short()).to_uppercase()
    }

    fn stem(&self) -> String {
        Path::new(&self.source)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// 从 .slang 源码里找出所有 `[shader("阶段")]` 标注的入口函数。
fn entries_in(source: &str, text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("[shader(\"") {
        let after = &rest[at + 9..];
        let Some(quote) = after.find('"') else { break };
        let stage = after[..quote].to_string();
        // 标注之后的第一个 `名字(` 就是入口函数
        let body = &after[quote..];
        let name = body
            .split('\n')
            .skip(1)
            .find_map(|line| {
                let paren = line.find('(')?;
                let head = &line[..paren];
                let ident = head.rsplit([' ', '\t', '*', '&']).next()?;
                (!ident.is_empty() && ident.chars().all(|c| c.is_alphanumeric() || c == '_'))
                    .then(|| ident.to_string())
            })
            .unwrap_or_default();
        if !name.is_empty() {
            out.push(Entry {
                source: source.to_string(),
                name,
                stage,
            });
        }
        rest = body;
    }
    out
}

/// 扫 `shaders/` 下的所有 .slang，收集入口点。
fn shader_entries(root: &Path) -> Result<Vec<Entry>> {
    let dir = root.join("shaders");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for path in crate::fs::read_dir(&dir)? {
        if path.extension().and_then(|e| e.to_str()) != Some("slang") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let text = crate::fs::read_to_string(&path)?;
        entries.extend(entries_in(&format!("shaders/{name}"), &text));
    }

    // 头文件名由「文件名 + 阶段」拼出来，同一个文件里同阶段有两个入口就会撞。
    let mut seen = BTreeSet::new();
    for entry in &entries {
        if !seen.insert(entry.header()) {
            return Err(Error::new(
                Code::BadManifest,
                format!(
                    "{} 里有多个 {} 阶段的入口，生成的头文件名 {} 会冲突",
                    entry.source,
                    entry.stage,
                    entry.header()
                ),
                "把其中一个挪到另一个 .slang 文件里",
            ));
        }
    }
    entries.sort_by(|a, b| (&a.source, &a.name).cmp(&(&b.source, &b.name)));
    Ok(entries)
}

/// 生成 `target/CMakeLists.txt` 和它旁边的 cmake 模块。
pub fn cmake(project: &Project) -> Result<()> {
    let target = project.root.join("target");
    let entries = shader_entries(&project.root)?;

    let mut s = String::new();
    let name = &project.name;
    // 注意别叫 version：下面 format! 里 {version} 是具名实参，指 vkx 自己的版本。
    let project_version = &project.version;
    // 这个文件是每次构建重新生成的，写绝对路径不会跟着仓库跑到别人机器上。
    // 路径要过一道 cmake_path()：反斜杠在 CMake 字符串里是转义符。
    let sdk_libs = crate::toolchain::cmake_path(&crate::install::sdk_dir().join("libs"));
    // 宿主平台的名字。Android / iOS 那两支由下面的 CMake 自己挑，因为交叉编译
    // 的目标在生成这个文件时还不知道，要等 CMake 配置期看 ANDROID / IOS 变量。
    let host_dir = crate::install::host()?.name();
    s.push_str(&format!(
        "\
# 由 vkx {version} 从 ../vkx.toml 生成。别手改这个文件——下次构建会覆盖掉。
#
# 要加 vkx.toml 表达不了的东西，写进 [build] cmake_include 指向的那个文件，
# 它会在本文件末尾被 include 进来。
#
# 平台由 vkx 通过命令行告诉 CMake，脚本里用这两个变量分支：
#   ANDROID  为真时在交叉编译 Android
#   IOS      为真时在交叉编译 iOS

cmake_minimum_required(VERSION 3.24)
project({name} LANGUAGES C CXX)

set(VKX_ROOT \"${{CMAKE_CURRENT_SOURCE_DIR}}/..\")

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_C_STANDARD 11)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Debug)
endif()

list(APPEND CMAKE_MODULE_PATH \"${{CMAKE_CURRENT_SOURCE_DIR}}/cmake\")
include(FetchContent)
include(VkxShaders)

# SDK 里那份预编译的库。每个 target 一份，因为库是编好的二进制，换个目标就
# 换一份——头文件也一起分开：SDL_build_config.h 这类是按目标生成的，共用会错。
#
# 目标在这里才定得下来：桌面就是跑 vkx 的这台机器，Android 和 iOS 是交叉编译，
# 要等 CMake 拿到 ANDROID / IOS 变量才知道。
set(VKX_SDK_LIBS_ROOT \"{sdk_libs}\" CACHE PATH \"\")
if(ANDROID)
    # ANDROID_ABI 是 NDK 的叫法，转成我们自己的 target 名。
    if(ANDROID_ABI STREQUAL \"arm64-v8a\")
        set(VKX_SDK_LIBS \"${{VKX_SDK_LIBS_ROOT}}/android-arm64\")
    else()
        set(VKX_SDK_LIBS \"${{VKX_SDK_LIBS_ROOT}}/android-x64\")
    endif()
elseif(IOS)
    set(VKX_SDK_LIBS \"${{VKX_SDK_LIBS_ROOT}}/ios-arm64\")
else()
    set(VKX_SDK_LIBS \"${{VKX_SDK_LIBS_ROOT}}/{host_dir}\")
endif()
if(EXISTS \"${{VKX_SDK_LIBS}}\")
    list(APPEND CMAKE_PREFIX_PATH \"${{VKX_SDK_LIBS}}\")
endif()

set(VKX_SDL_TAG \"release-3.4.14\" CACHE STRING \"\")
set(VKX_VULKAN_HEADERS_TAG \"v1.4.313\" CACHE STRING \"\")
set(VKX_VOLK_TAG \"1.4.304\" CACHE STRING \"\")

# ---------------------------------------------------------------------------
# 依赖
# ---------------------------------------------------------------------------

function(vkx_fetch_sdl)
    message(STATUS \"vkx: 从源码构建 SDL3 (${{VKX_SDL_TAG}})\")
    set(SDL_SHARED OFF CACHE BOOL \"\" FORCE)
    set(SDL_STATIC ON CACHE BOOL \"\" FORCE)
    set(SDL_TEST_LIBRARY OFF CACHE BOOL \"\" FORCE)
    set(SDL_EXAMPLES OFF CACHE BOOL \"\" FORCE)
    set(SDL_INSTALL OFF CACHE BOOL \"\" FORCE)
    FetchContent_Declare(SDL3
        GIT_REPOSITORY https://github.com/libsdl-org/SDL.git
        GIT_TAG ${{VKX_SDL_TAG}}
        GIT_SHALLOW TRUE)
    FetchContent_MakeAvailable(SDL3)
endfunction()

if(ANDROID)
    # Gradle 已经通过 prefab 把官方 .aar 里的 SDL3 准备好了。
    find_package(SDL3 REQUIRED CONFIG)
elseif(IOS OR FETCHCONTENT_SOURCE_DIR_SDL3)
    # iOS 上系统装的是 macOS 版，用不了，只能交叉编译。
    vkx_fetch_sdl()
else()
    find_package(SDL3 3.2 QUIET CONFIG)
    if(NOT SDL3_FOUND)
        vkx_fetch_sdl()
    endif()
endif()

# SDK 里带了，就不用出网。VulkanHeaders 的 config 装在 share/cmake 下，
# find_package 顺着 CMAKE_PREFIX_PATH 找得到。
find_package(VulkanHeaders QUIET CONFIG)
if(NOT VulkanHeaders_FOUND)
    message(STATUS \"vkx: SDK 里没有 Vulkan-Headers，从源码取\")
    FetchContent_Declare(VulkanHeaders
        GIT_REPOSITORY https://github.com/KhronosGroup/Vulkan-Headers.git
        GIT_TAG ${{VKX_VULKAN_HEADERS_TAG}}
        GIT_SHALLOW TRUE)
    FetchContent_MakeAvailable(VulkanHeaders)
endif()

# volk 在运行期加载 Vulkan 函数指针，链接期就不必依赖 loader。
# iOS 不需要它：那边静态链接 MoltenVK，函数本来就在二进制里。
#
# volk 就一个 .c，自己编，不用 find_package——它自带的 CMake 包会把打包机器
# 的绝对路径写进 INTERFACE_INCLUDE_DIRECTORIES，在读者机器上是不存在的路径。
if(NOT IOS)
    if(EXISTS \"${{VKX_SDK_LIBS}}/include/volk.c\")
        add_library(volk STATIC \"${{VKX_SDK_LIBS}}/include/volk.c\")
        target_include_directories(volk PUBLIC \"${{VKX_SDK_LIBS}}/include\")
    else()
        message(STATUS \"vkx: SDK 里没有 volk，从源码取\")
        FetchContent_Declare(volk
            GIT_REPOSITORY https://github.com/zeux/volk.git
            GIT_TAG ${{VKX_VOLK_TAG}}
            GIT_SHALLOW TRUE)
        set(VOLK_PULL_IN_VULKAN OFF CACHE BOOL \"\" FORCE)
        FetchContent_MakeAvailable(volk)
    endif()
    target_link_libraries(volk PUBLIC Vulkan::Headers)
    if(NOT WIN32)
        target_link_libraries(volk PUBLIC ${{CMAKE_DL_LIBS}})
    endif()
endif()

# ---------------------------------------------------------------------------
# 目标
# ---------------------------------------------------------------------------

# 源码自动收集：往 src/ 里加文件不用改任何配置。
# CONFIGURE_DEPENDS 让 ninja 在文件增删时自动重新 configure。
file(GLOB_RECURSE VKX_SOURCES CONFIGURE_DEPENDS
    \"${{VKX_ROOT}}/src/*.c\"
    \"${{VKX_ROOT}}/src/*.cc\"
    \"${{VKX_ROOT}}/src/*.cpp\"
    \"${{VKX_ROOT}}/src/*.cxx\")
if(NOT VKX_SOURCES)
    message(FATAL_ERROR \"src/ 下一个源文件都没有\")
endif()

if(ANDROID)
    # Android 上没有可执行文件：Java 层的 SDLActivity 去加载名为 main 的共享库。
    add_library(main SHARED ${{VKX_SOURCES}})
    set(VKX_TARGET main)
else()
    add_executable({name} ${{VKX_SOURCES}})
    set(VKX_TARGET {name})
endif()

# 头文件按 \"gpu/gpu.h\" 这样从 src/ 起写，一眼看得出属于哪一层。
target_include_directories(${{VKX_TARGET}} PRIVATE \"${{VKX_ROOT}}/src\")

# 只有头文件的库：GLM、cpp-httplib、stb 都摊在 SDK 的 include/ 下。
if(EXISTS \"${{VKX_SDK_LIBS}}/include\")
    target_include_directories(${{VKX_TARGET}} PRIVATE \"${{VKX_SDK_LIBS}}/include\")
endif()

# VKX_DEBUG 只在 Debug 配置下定义，用来开关校验层。
target_compile_definitions(${{VKX_TARGET}} PRIVATE $<$<CONFIG:Debug>:VKX_DEBUG=1>)

# macOS：可执行文件放到构建目录根上，vkx run 按这个位置找它。
if(APPLE AND NOT IOS)
    set_target_properties(${{VKX_TARGET}} PROPERTIES
        RUNTIME_OUTPUT_DIRECTORY \"${{CMAKE_BINARY_DIR}}\")
endif()

if(IOS)
    set_target_properties(${{VKX_TARGET}} PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_INFO_PLIST \"${{VKX_ROOT}}/ios/Info.plist\"
        # 版本号来自 vkx.toml。CMake 会把 ios/Info.plist 当模版处理，
        # 里面的 ${{MACOSX_BUNDLE_*}} 由这两行填掉。
        MACOSX_BUNDLE_SHORT_VERSION_STRING \"{project_version}\"
        MACOSX_BUNDLE_BUNDLE_VERSION \"{project_version}\"
        XCODE_ATTRIBUTE_PRODUCT_BUNDLE_IDENTIFIER \"{package_id}\"
        XCODE_ATTRIBUTE_TARGETED_DEVICE_FAMILY \"1,2\")

    if(NOT VKX_MOLTENVK_LIB)
        message(FATAL_ERROR
            \"iOS 构建需要 -DVKX_MOLTENVK_LIB=<libMoltenVK.a 的路径>。\\n\"
            \"正常情况下 `vkx build --target ios` 会自动传入。\")
    endif()

    # iOS 上没有 Vulkan loader，Vulkan 由静态库 MoltenVK 提供。
    # -force_load 必须加：代码里没有一处直接引用 vkGetInstanceProcAddr
    #（SDL 是运行期 dlsym 找它的），否则链接器会把整个 MoltenVK 丢掉。
    target_link_libraries(${{VKX_TARGET}} PRIVATE
        \"-force_load ${{VKX_MOLTENVK_LIB}}\"
        \"-framework Metal\"
        \"-framework Foundation\"
        \"-framework QuartzCore\"
        \"-framework CoreGraphics\"
        \"-framework IOSurface\"
        \"-framework UIKit\")
endif()
",
        version = env!("CARGO_PKG_VERSION"),
        name = name,
        package_id = project.package_id,
    ));

    // ---- vkx.toml 的 dependencies ----
    //
    // 全部是预编译好的（离线包里的 libs 组件），所以这里只做两件事：
    // find_package 找到它，target_link_libraries 链上去。开关不影响构建时间。
    s.push_str(
        "\n\
# ---------------------------------------------------------------------------\n\
# 依赖（vkx.toml 的 dependencies）\n\
# ---------------------------------------------------------------------------\n\
# 全是预编译的，改这个列表不影响构建时间，只影响链哪些库、暴露哪些头文件。\n\
# 纯头文件的那几个（GLM、cpp-httplib、stb）不需要 find_package，\n\
# 声明了就只是把 include 路径放出来——上面那句 target_include_directories\n\
# 已经把整个 include/ 加进去了。\n",
    );
    if project.dependencies.is_empty() {
        s.push_str("# （vkx.toml 里没有声明任何依赖）\n");
    }
    for name in &project.dependencies {
        let Some(dep) = crate::project::find_dependency(name) else {
            continue;
        };
        s.push_str(&format!("\n# {}\n", dep.about));

        // Vulkan 那一支特殊：volk 是我们自己编的（见上面），而 iOS 上
        // 静态链接的 MoltenVK 自带全部函数，不走 volk。
        if dep.name == "Vulkan" {
            s.push_str(
                "if(IOS)\n\
                 \x20   # 静态链接的 MoltenVK 已提供全部函数，用头文件里的原型直接调用。\n\
                 \x20   target_link_libraries(${VKX_TARGET} PRIVATE Vulkan::Headers)\n\
                 \x20   target_compile_definitions(${VKX_TARGET} PRIVATE VKX_STATIC_VULKAN=1)\n\
                 else()\n\
                 \x20   # VK_NO_PROTOTYPES 去掉头文件里的声明，改由 volk 提供同名函数指针。\n\
                 \x20   target_link_libraries(${VKX_TARGET} PRIVATE volk)\n\
                 \x20   target_compile_definitions(${VKX_TARGET} PRIVATE VK_NO_PROTOTYPES)\n\
                 endif()\n",
            );
            continue;
        }

        // guard 非空的包在条件里：比如 OpenSSL 只有非 Windows 才编了
        // （Windows 上 GameNetworkingSockets 用系统的 BCrypt）。
        let (open, close, indent) = if dep.guard.is_empty() {
            (String::new(), String::new(), "")
        } else {
            (
                format!("if({})\n", dep.guard),
                "endif()\n".to_string(),
                "    ",
            )
        };
        s.push_str(&open);
        if !dep.package.is_empty() {
            // REQUIRED：找不到就当场停，而不是等到链接期报一堆 undefined symbol。
            s.push_str(&format!(
                "{indent}find_package({} REQUIRED{})\n",
                dep.package,
                if dep.config_mode { " CONFIG" } else { "" }
            ));
        }
        if !dep.targets.is_empty() {
            s.push_str(&format!(
                "{indent}target_link_libraries(${{VKX_TARGET}} PRIVATE {})\n",
                dep.targets.join(" ")
            ));
        }
        s.push_str(&close);
    }

    // ---- 着色器 ----
    s.push_str(
        "\n\
# ---------------------------------------------------------------------------\n\
# 着色器：Slang -> SPIR-V -> 嵌入头文件\n\
# ---------------------------------------------------------------------------\n\
# 下面这些规则是扫 shaders/*.slang 里的 [shader(\"...\")] 标注自动生成的。\n\
# 加一个入口点就自动多一条，不用改配置。\n",
    );
    if entries.is_empty() {
        s.push_str("# （shaders/ 下没有找到入口点）\n");
    }
    for entry in &entries {
        s.push_str(&format!(
            "\nvkx_add_slang_shader(${{VKX_TARGET}}\n    \
             SOURCE \"${{VKX_ROOT}}/{source}\"\n    \
             ENTRY  {name}\n    \
             STAGE  {stage}\n    \
             VAR    {var}\n    \
             HEADER {header})\n",
            source = entry.source,
            name = entry.name,
            stage = entry.stage,
            var = entry.var(),
            header = entry.header(),
        ));
    }

    s.push_str(
        "\n\
# ---------------------------------------------------------------------------\n\
# 逃生舱：vkx.toml 表达不了的东西写在工程根目录的 extra.cmake 里\n\
# ---------------------------------------------------------------------------\n\
include(\"${VKX_ROOT}/extra.cmake\" OPTIONAL)\n",
    );

    crate::fs::write(&target.join("CMakeLists.txt"), &s)?;
    write_modules(&target)?;
    write_presets(project)?;
    Ok(())
}

/// cmake 模块跟着生成物走，工程根目录下不留 CMake 相关的东西。
fn write_modules(target: &Path) -> Result<()> {
    crate::fs::write(
        &target.join("cmake/VkxShaders.cmake"),
        include_str!("../assets/cmake/VkxShaders.cmake"),
    )?;
    crate::fs::write(
        &target.join("cmake/VkxEmbed.cmake"),
        include_str!("../assets/cmake/VkxEmbed.cmake"),
    )?;
    Ok(())
}

/// 根目录放一份 CMakePresets.json，IDE 打开工程目录就能认出构建配置。
fn write_presets(project: &Project) -> Result<()> {
    let presets = r#"{
  "version": 6,
  "comment": "由 vkx 生成。IDE 靠它找到 target/ 下的 CMakeLists.txt。",
  "configurePresets": [
    {
      "name": "debug",
      "displayName": "Debug",
      "generator": "Ninja",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Debug" },
      "binaryDir": "${sourceDir}/target/debug"
    },
    {
      "name": "release",
      "displayName": "Release",
      "generator": "Ninja",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" },
      "binaryDir": "${sourceDir}/target/release"
    }
  ],
  "buildPresets": [
    { "name": "debug", "configurePreset": "debug" },
    { "name": "release", "configurePreset": "release" }
  ]
}
"#;
    crate::fs::write(&project.root.join("CMakePresets.json"), presets)
}

#[cfg(test)]
mod tests {
    use crate::project::Project;

    fn 造一个工程(dir: &std::path::Path, version: &str) -> Project {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("shaders")).unwrap();
        std::fs::write(dir.join("src/main.cpp"), "int main(){return 0;}").unwrap();
        Project {
            root: dir.to_path_buf(),
            name: "app".into(),
            package_id: "tech.yinli.app".into(),
            version: version.into(),
            dependencies: vec!["SDL3".into(), "Vulkan".into()],
        }
    }

    /// 生成一份 CMakeLists 并读回来。
    ///
    /// tag 是每个测试自己的名字：测试是并行跑的，共用目录会互相把对方删掉。
    fn 生成(tag: &str, version: &str) -> String {
        let dir = std::env::temp_dir().join(format!("vkxgen-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        let project = 造一个工程(&dir, version);
        assert!(super::cmake(&project).is_ok(), "生成 CMakeLists 失败");
        let text = std::fs::read_to_string(dir.join("target/CMakeLists.txt")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        text
    }

    // 生成的 CMakeLists 里有两个「版本」：vkx 自己的（写在抬头注释里）和工程的
    // （刻进 iOS bundle）。它们在 format! 里差一点就同名——具名实参会静默盖掉
    // 同名局部变量，结果是 iOS 包的版本号变成 vkx 的版本号，编译和构建全都不报错，
    // 只有掏出产物里的 Info.plist 才看得出来。这条测试就是防这个。
    #[test]
    fn ios_版本号取工程的而不是_vkx_的() {
        let text = 生成("ios-version", "2.7.3");
        assert!(
            text.contains("MACOSX_BUNDLE_SHORT_VERSION_STRING \"2.7.3\""),
            "iOS 的 short version 没取到工程版本"
        );
        assert!(
            text.contains("MACOSX_BUNDLE_BUNDLE_VERSION \"2.7.3\""),
            "iOS 的 bundle version 没取到工程版本"
        );
        let vkx = env!("CARGO_PKG_VERSION");
        assert!(
            !text.contains(&format!("MACOSX_BUNDLE_BUNDLE_VERSION \"{vkx}\"")),
            "iOS 版本号写成了 vkx 自己的版本（{vkx}）——多半是被 format! 的具名实参遮蔽了"
        );
        // 抬头那句仍然该是 vkx 的版本，别把两个搞反了
        assert!(
            text.contains(&format!("由 vkx {vkx} ")),
            "抬头的 vkx 版本丢了"
        );
    }

    // 预编译库按 target 分目录之后，三条分支都得能拼出路径。
    #[test]
    fn 三种目标各自选到自己的库目录() {
        let text = 生成("libs-dirs", "0.1.0");
        assert!(text.contains("VKX_SDK_LIBS_ROOT"), "没有 libs 根变量");
        for 片段 in [
            "${VKX_SDK_LIBS_ROOT}/android-arm64",
            "${VKX_SDK_LIBS_ROOT}/android-x64",
            "${VKX_SDK_LIBS_ROOT}/ios-arm64",
        ] {
            assert!(text.contains(片段), "缺少分支：{片段}");
        }
        // 桌面那支用的是宿主名，跟着这台机器走
        let Ok(host) = crate::install::host() else {
            return; // 不认识的开发平台，这条不适用
        };
        let host = host.name();
        assert!(
            text.contains(&format!("${{VKX_SDK_LIBS_ROOT}}/{host}")),
            "桌面分支没指向 libs/{host}"
        );
    }

    // dependencies 决定链哪些库。写错了不会当场报错——CMakeLists 里少一行
    // target_link_libraries，要到链接期才炸成一堆 undefined symbol。
    #[test]
    fn 声明的依赖会被链进去() {
        let text = 生成("deps", "0.1.0");
        // 默认模版声明了 SDL3 和 Vulkan
        assert!(
            text.contains("find_package(SDL3 REQUIRED CONFIG)"),
            "SDL3 没 find_package"
        );
        assert!(text.contains("SDL3::SDL3"), "SDL3 没链上");
        // Vulkan 那支是特殊的：iOS 用 Vulkan::Headers，其余用自己编的 volk
        assert!(text.contains("PRIVATE volk"), "volk 没链上");
        assert!(
            text.contains("Vulkan::Headers"),
            "iOS 那支没有 Vulkan::Headers"
        );
        // 没声明的不该出现
        assert!(!text.contains("Jolt::Jolt"), "没声明 Jolt 却链了它");
        assert!(
            !text.contains("GameNetworkingSockets::"),
            "没声明 GNS 却链了它"
        );
    }

    // 着色器规则是扫 shaders/*.slang 生成的。它整段丢了的话，CMake 照样配置成功，
    // 要到编译期才报「triangle_frag.spv.h file not found」——那个错离真正的原因
    // （生成器少写了一段）隔得很远。我改依赖那段时就把它误删过一次。
    #[test]
    fn 着色器规则要在生成的_cmakelists_里() {
        let dir = std::env::temp_dir().join("vkxgen-shaders");
        let _ = std::fs::remove_dir_all(&dir);
        let project = 造一个工程(&dir, "0.1.0");
        // 放一个带两个入口点的着色器
        std::fs::write(
            dir.join("shaders/rect.slang"),
            "[shader(\"vertex\")]\nfloat4 vertex_main() : SV_Position { return 0; }\n             [shader(\"fragment\")]\nfloat4 fragment_main() : SV_Target { return 0; }\n",
        )
        .unwrap();
        assert!(super::cmake(&project).is_ok());
        let text = std::fs::read_to_string(dir.join("target/CMakeLists.txt")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            text.matches("vkx_add_slang_shader").count(),
            2,
            "两个入口点该生成两条规则"
        );
        assert!(text.contains("ENTRY  vertex_main"));
        assert!(text.contains("ENTRY  fragment_main"));
    }

    // 模版里的占位符是 {{}}，CMake 的是 ${}；生成出来的文件里不该再有前者。
    #[test]
    fn 生成的_cmakelists_没有未替换的占位符() {
        let text = 生成("no-placeholder", "0.1.0");
        assert!(!text.contains("{{"), "生成的 CMakeLists 里还有 {{{{");
    }
}
