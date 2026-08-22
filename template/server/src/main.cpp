// {{PROJECT_NAME}} —— 一个 HTTP 服务端。
//
// 整个程序就这一个文件。路由在 main() 里注册，加一条就是加一个 lambda。
//
// 用的是 cpp-httplib：一个纯头文件的 HTTP 库，已经在离线包里，
// vkx.toml 的 dependencies 里声明了 "cpp-httplib" 就能直接 #include。
#include <httplib.h>

#include <cstdio>
#include <cstdlib>
#include <string>

namespace {

// 监听哪个地址、哪个端口。
//
// 绑 127.0.0.1 意味着只有本机连得上。要让局域网里的手机连过来，
// 改成 "0.0.0.0"——那样任何能访问到这台机器的人都能连，自己心里有数。
constexpr const char* HOST = "127.0.0.1";
constexpr int PORT = 8080;

}  // namespace

int main()
{
    httplib::Server server;

    // GET /hello -> Hello
    server.Get("/hello", [](const httplib::Request&, httplib::Response& res) {
        res.set_content("Hello", "text/plain");
    });

    // GET /sum/<a>/<b> -> a + b
    //
    // 尖括号里是路径参数，httplib 会把它们放进 req.path_params。
    // 用 long long 而不是 int：int 在两个大数相加时会静默溢出，
    // 而这里的输入是从网上来的，不能假设它规矩。
    server.Get(R"(/sum/(-?\d+)/(-?\d+))", [](const httplib::Request& req, httplib::Response& res) {
        try {
            const long long a = std::stoll(req.matches[1]);
            const long long b = std::stoll(req.matches[2]);
            res.set_content(std::to_string(a + b), "text/plain");
        } catch (const std::exception&) {
            // 数字太长会抛 out_of_range。返回 400 而不是让进程崩掉：
            // 服务端的每一条输入都得当成可能是恶意的。
            res.status = 400;
            res.set_content("数字超出范围", "text/plain");
        }
    });

    // 没匹配上的路径。不写这个的话 httplib 会返回一个空的 404，
    // 调试时看不出是路由没写对还是服务根本没起来。
    server.set_error_handler([](const httplib::Request& req, httplib::Response& res) {
        char body[256];
        std::snprintf(body, sizeof(body), "%d %s 没有这个路由\n", res.status, req.path.c_str());
        res.set_content(body, "text/plain");
    });

    std::printf("%s 监听 http://%s:%d\n", "{{PROJECT_NAME}}", HOST, PORT);
    std::printf("  curl http://%s:%d/hello\n", HOST, PORT);
    std::printf("  curl http://%s:%d/sum/3/4\n", HOST, PORT);
    std::fflush(stdout);

    if (!server.listen(HOST, PORT)) {
        std::fprintf(stderr, "监听失败：%s:%d 可能已经被占用\n", HOST, PORT);
        return 1;
    }
    return 0;
}
