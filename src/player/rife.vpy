# bilibili-tui 自动生成 — 请勿手改 (每次播放按配置重写本文件)
# RIFE 光流真补帧: vsrife (github.com/HolyWu/vs-rife) + TensorRT (torch_tensorrt)
# 运行环境: ~/.local/share/bilibili-tui/rife-venv (应用引导安装, 无需 sudo)
#
# mpv vf=vapoursynth 契约: 读取 video_in, 最终 set_output(); 输出帧由
# VapourSynth 自动携带 _DurationNum/_DurationDen 时间戳 → mpv 按倍增后
# 的 fps 呈现 (与 bwdif 倍频同机制); seek 时 mpv 会重载本脚本。
import os
import sys

# mpv 内嵌 python 是系统解释器, 把 venv 的 site-packages 挂进来
sys.path.insert(0, "__SITE_PKGS__")

import vapoursynth as vs
from vsrife import rife

core = vs.core

MULT = __MULTI__    # 应用注入: factor_num/factor_den = 输出 fps / 输入 fps
MODEL = __MODEL__   # 应用注入: vsrife 模型版本 (4.25 / 4.22.lite 等)
SC_THRESHOLD = 0.15  # 场景切换阈值 (帧间归一化差, 同 vsrife 默认)
TRT_CACHE = os.path.expanduser("~/.cache/bilibili-tui/rife-engine")
os.makedirs(TRT_CACHE, exist_ok=True)

# RIFE 在 RGB 上推理。RGBH = FP16 (vsrife 精度由像素格式决定,
# RGBS 是 FP32 慢一倍); 矩阵/范围进出一致 (bt709/limited) 往返抵消,
# 中间误差只影响光流估计的数值。
_in = video_in
_out_format = _in.format
rgb = _in.resize.Bicubic(format=vs.RGBH, matrix_in_s="709", range_in_s="limited")

# 场景切换检测: 本机没有 misc 插件 (vsrife 内置 sc_detect 依赖它), 用纯
# std 等价实现 — _SceneChangeNext[n] = 归一化帧差(f_n, f_{n+1}) > 阈值,
# PlaneStats 的 PlaneStatsDiff 正是 0..1 归一化平均绝对差 (misc 同源)。
# Trim 的 length 用的是 mpv 虚拟长度, 流式惰性求值; 向前只多取 1 帧,
# 在 mpv 的 buffered-frames 窗口内。随后给 rife 传 sc_threshold=None
# (vsrife 约定 = 使用已置好的属性), 从而绕开 misc 依赖。
_n = rgb.num_frames
if _n >= 2:
    _a = core.std.Trim(rgb, first=0, length=_n - 1)
    _b = core.std.Trim(rgb, first=1, length=_n - 1)
    _stats = core.std.PlaneStats(_a, _b)

    # 注意: VS 以关键字 (n=, f=) 回调 selector, 参数名必须是 n/f。
    def _set_sc(n, f):
        fout = f[0].copy()
        fout.props["_SceneChangeNext"] = int(
            float(f[1].props.get("PlaneStatsDiff", 0.0)) > SC_THRESHOLD
        )
        return fout

    rgb = core.std.ModifyFrame(_a, clips=[_a, _stats], selector=_set_sc)

out = rife(
    rgb,
    device_index=0,
    model=MODEL,
    factor_num=MULT,
    factor_den=1,
    sc=True,                 # 硬切镜不插帧, 避免转场鬼影
    sc_threshold=None,       # 用上面已置好的 _SceneChangeNext (绕开 misc 插件)
    trt=True,                # TensorRT (pypi wheel 自带运行时, 不依赖系统 CUDA)
    trt_static_shape=False,  # 动态形状: 一套引擎覆盖范围内所有分辨率
    # vsrife 默认 opt/max=1920x1080, 超出的 4K 源会因引擎形状不符被拒
    # (mpv 会停用滤镜继续无补帧播放)。显式放宽到 4K, 代价是引擎稍大。
    trt_min_shape=[128, 128],
    trt_opt_shape=[1920, 1080],
    trt_max_shape=[3840, 2160],
    trt_cache_dir=TRT_CACHE,
)
out.resize.Bicubic(
    format=_out_format.id, matrix_s="709", range_s="limited"
).set_output()
