fn main() {
    // 模版是用 include_dir! 在编译期内嵌进二进制的，
    // 不显式声明依赖的话，改了模版 cargo 不会重新编译。
    println!("cargo:rerun-if-changed=template");
}
