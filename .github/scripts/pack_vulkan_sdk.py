#!/usr/bin/env python3
"""从一棵 Vulkan SDK（或 ValidationLayers 的构建目录）里挑出需要的文件，
重打包成 vkx 的统一格式。

由 .github/workflows/vulkan-sdk.yml 调用，产出：

    <out>/vulkan-sdk-<版本>-<平台>.tar.gz
    <out>/vulkan-sdk-<版本>-<平台>.tar.gz.sha256

包内布局（解开后就是 ~/.vkx/tools/vulkan 该有的内容）：

    lib/libvulkan.1.dylib                                  仅 macOS
    lib/libMoltenVK.dylib                                  仅 macOS
    lib/libVkLayer_khronos_validation.{dylib,so,dll}
    share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json
    share/vulkan/icd.d/MoltenVK_icd.json                   仅 macOS

上游三个平台的目录结构互不相同（macOS/lib、x86_64/lib、Bin），所以这里不写死
路径，一律按文件名递归查找，并把找到的路径打印出来——CI 日志即是记录。

两个 json 里的 library_path 会被统一改写成 ../../../lib/<文件名>。Vulkan loader
按 json 自身所在目录解析相对路径，改写之后整棵树可以整体搬动，运行时不再需要
任何库搜索路径（macOS 上就不必设 DYLD_LIBRARY_PATH，那个变量会被 SIP 剥掉）。
"""

import argparse
import hashlib
import json
import os
import shutil
import sys
import tarfile
from pathlib import Path

# 每个平台要挑哪些文件。
#   键是包内的目标相对路径，值是要在上游树里找的文件名。
# Linux 和 Windows 不带 loader 和 ICD：那两个平台的 loader 由显卡驱动提供，
# 只有 macOS 什么都没有，得连 loader 带 MoltenVK 一起给。
LAYOUT = {
    "macos-universal": {
        "lib/libvulkan.1.dylib": "libvulkan.1.dylib",
        "lib/libMoltenVK.dylib": "libMoltenVK.dylib",
        "lib/libVkLayer_khronos_validation.dylib": "libVkLayer_khronos_validation.dylib",
        "share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json": "VkLayer_khronos_validation.json",
        "share/vulkan/icd.d/MoltenVK_icd.json": "MoltenVK_icd.json",
    },
    "linux-x86_64": {
        "lib/libVkLayer_khronos_validation.so": "libVkLayer_khronos_validation.so",
        "share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json": "VkLayer_khronos_validation.json",
    },
    "linux-aarch64": {
        "lib/libVkLayer_khronos_validation.so": "libVkLayer_khronos_validation.so",
        "share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json": "VkLayer_khronos_validation.json",
    },
    "windows-x86_64": {
        "lib/VkLayer_khronos_validation.dll": "VkLayer_khronos_validation.dll",
        "share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json": "VkLayer_khronos_validation.json",
    },
    "windows-aarch64": {
        "lib/VkLayer_khronos_validation.dll": "VkLayer_khronos_validation.dll",
        "share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json": "VkLayer_khronos_validation.json",
    },
}

# 上游整包里塞了一堆我们不要的东西，命中这些片段的路径直接跳过，
# 免得挑到示例程序 app 包里那份同名的库。
EXCLUDE = ("/Applications/", ".app/", "/samples/", "/tests/", "/Templates/",
           "/external/", "/x86_64/lib/cmake/")


def find_one(root: Path, filename: str) -> Path:
    """在 root 下递归找 filename，返回最合适的一个。

    可能命中多份（比如 vkcube.app 里也带一份 loader），所以先排除掉黑名单里的
    路径，再按路径深度排序取最浅的那个——上游把正经产物放在顶层目录里。
    """
    hits = [p for p in root.rglob(filename)
            if not any(x in p.as_posix() for x in EXCLUDE)]
    if not hits:
        raise SystemExit(f"错误: 在 {root} 下找不到 {filename}")
    hits.sort(key=lambda p: (len(p.parts), p.as_posix()))
    if len(hits) > 1:
        print(f"    {filename} 命中 {len(hits)} 份，取最浅的一个：")
        for h in hits[:5]:
            print(f"      {'->' if h is hits[0] else '  '} {h.relative_to(root)}")
    return hits[0]


def rewrite_json(path: Path, key: str) -> None:
    """把 json 里的 library_path 改写成 ../../../lib/<文件名>。

    key 是顶层字段名：层用 "layer"，ICD 用 "ICD"。
    """
    data = json.loads(path.read_text())
    before = data[key]["library_path"]
    data[key]["library_path"] = f"../../../lib/{Path(before).name}"
    path.write_text(json.dumps(data, indent=4))
    print(f"    {path.name}: library_path {before} -> {data[key]['library_path']}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sdk-root", required=True)
    ap.add_argument("--platform", required=True)
    ap.add_argument("--version", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    root = Path(args.sdk_root).resolve()
    layout = LAYOUT.get(args.platform)
    if layout is None:
        raise SystemExit(f"错误: 未知平台 {args.platform}")

    stage = Path("stage").resolve()
    shutil.rmtree(stage, ignore_errors=True)

    print(f"==> 从 {root} 挑 {args.platform} 需要的文件")
    for dest_rel, filename in layout.items():
        src = find_one(root, filename)
        dest = stage / dest_rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        # 上游的 loader 是「真文件 + 两个软链接」，这里跟随软链接拷真文件，
        # 直接落成规范名字。运行时用的是绝对路径，不需要保留链接结构。
        shutil.copy(src, dest)
        print(f"    {src.relative_to(root)}  ->  {dest_rel}  ({dest.stat().st_size // 1024} KB)")

    print("==> 改写 json 里的库路径")
    rewrite_json(stage / "share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json", "layer")
    icd = stage / "share/vulkan/icd.d/MoltenVK_icd.json"
    if icd.exists():
        rewrite_json(icd, "ICD")

    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    name = f"vulkan-sdk-{args.version}-{args.platform}.tar.gz"
    archive = out / name

    print(f"==> 打包 {name}")
    with tarfile.open(archive, "w:gz") as tar:
        for item in sorted(stage.rglob("*")):
            if item.is_file():
                rel = item.relative_to(stage)
                # 清掉 uid/gid/mtime，让同样的输入产出同样的包。
                info = tar.gettarinfo(item, arcname=str(rel))
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mtime = 0
                with item.open("rb") as fh:
                    tar.addfile(info, fh)

    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (out / f"{name}.sha256").write_text(f"{digest}  {name}\n")
    print(f"    {archive.stat().st_size // 1048576} MB  sha256 {digest[:16]}...")
    return 0


if __name__ == "__main__":
    sys.exit(main())
