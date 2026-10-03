# bilibili-tui 自动生成 — 请勿手改 (每次播放按配置重写本文件)
# RIFE 光流真补帧: vsrife (github.com/HolyWu/vs-rife) + TensorRT (torch_tensorrt)
# 运行环境: ~/.local/share/bilibili-tui/rife-venv (应用引导安装, 无需 sudo)
#
# mpv vf=vapoursynth 契约: 读取 video_in, 最终 set_output(); 输出帧由
# VapourSynth 自动携带 _DurationNum/_DurationDen 时间戳 → mpv 按倍增后
# 的 fps 呈现 (与 bwdif 倍频同机制); seek 时 mpv 会重载本脚本。
from fractions import Fraction
import os
import sys

# mpv 内嵌 python 是系统解释器, 把 venv 的 site-packages 挂进来
sys.path.insert(0, "__SITE_PKGS__")

import vapoursynth as vs
from vsrife import rife

core = vs.core

MULT = __MULTI__    # 应用注入: 请求倍率上限 (输出 fps ≈ 输入 fps × N)
MODEL = __MODEL__   # 应用注入: vsrife 模型版本 (4.25 / 4.22.lite 等)
SCALE = __SCALE__   # 应用注入: 光流推理分辨率; "auto" = 1.0
SC_THRESHOLD = 0.15  # 场景切换阈值 (帧间归一化差, 同 vsrife 默认)
TRT_CACHE = os.path.expanduser("~/.cache/bilibili-tui/rife-engine")
os.makedirs(TRT_CACHE, exist_ok=True)

# ── 自适应算力决策 (瓶颈实测标定, RTX 4060 Laptop) ─────────────────────
# vsrife 的真实瓶颈是逐帧 Python↔GPU 数据搬运 (RGBH 整帧 H2D/D2H +
# numpy 序列化), 与光流分辨率无关 (scale=0.5 实测无收益), 按像素反比
# 外推输出帧吞吐上限: 1080p ≈ 102 fps, 720p ≈ 273 fps。mpv 滤镜包装
# 管线另占约 20%, 按 0.8×上限留余量:
#   1) 请求倍率放不下 → 自动逐档降 (2 → 1.75 → 1.5 → 1.25);
#   2) 连 ×1.25 都放不下 (如 4K 源) → 跳过光流直通原帧, 由 mpv
#      display-resample 混合补齐 —— 等效自动回落混合模式, 宁可混合
#      也不容忍半速播放 (用户实测"十分卡顿"的根因)。
# 决策在 RGB 转换之前做: skip 路径直通 video_in, 否则 4K 源光
# YUV→RGBH 的 CPU 转换就要吃掉 36% 实时性 (实测 0.64)。
# fps 优先取 mpv 注入的 container_fps (流媒体上 video_in 自身 fps 不可靠)。
_fps = float(container_fps) if container_fps and container_fps > 0 else 0.0
if not _fps or _fps != _fps:
    _fps = video_in.fps_num / video_in.fps_den if video_in.fps_den else 24.0
SCALE_USED = SCALE if SCALE != "auto" else 1.0
# 经验容量式 (mpv 管线实测标定): (输入fps + 输出fps) × 像素 ≤ 210M/s。
# 依据: 1080p24×2 (72×2.07M=149M) 实时 0.998; 1080p60×1.25 (135×2.07M=
# 279M) 只有 0.62 (输入帧的 hwdownload/resize 与输出帧搬运同样计费,
# 独立测的 102 输出fps 不适用于 mpv 管线); 720p60×2 (180×0.92M=166M)
# 实时 0.999; 4K24 连输入都放不下 (24×8.29M=199M 无输出余量)。
_MP = float(video_in.width * video_in.height)
_budget_out = 210.0e6 / _MP - _fps  # 可承载的输出帧率
_used = float(MULT)
while _used > 1.25 and _fps * _used > _budget_out:
    _used -= 0.25
skip_rife = _fps * 1.25 > _budget_out  # 连 ×1.25 都放不下 → 跳过

try:  # 诊断转储 (每帧求值覆盖一次, 便于排查决策)
    with open("/tmp/rife_decision.txt", "w") as _dbg:
        _dbg.write(
            f"container_fps={container_fps} vfps={video_in.fps_num}/{video_in.fps_den}"
            f" res={video_in.width}x{video_in.height} budget_out={_budget_out:.0f}"
            f" fps={_fps:.1f} used={_used} skip={skip_rife}\n"
        )
except Exception:
    pass

if skip_rife:
    out = video_in
else:
    # RIFE 在 RGB 上推理。RGBH = FP16 (vsrife 精度由像素格式决定,
    # RGBS 是 FP32 慢一倍); 矩阵/范围进出一致 (bt709/limited) 往返抵消。
    _in = video_in
    _out_format = _in.format
    rgb = _in.resize.Bicubic(format=vs.RGBH, matrix_in_s="709", range_in_s="limited")

    # 场景切换检测: 本机没有 misc 插件 (vsrife 内置 sc_detect 依赖它),
    # 用纯 std 等价实现 — _SceneChangeNext[n] = 归一化帧差(f_n,f_{n+1})
    # > 阈值; PlaneStatsDiff 正是 0..1 归一化平均绝对差 (misc 同源)。
    # Trim 用 mpv 虚拟长度, 流式惰性求值; 向前只多取 1 帧 (在 mpv
    # buffered-frames 窗口内)。rife 传 sc_threshold=None (vsrife 约定 =
    # 使用已置好的属性), 绕开 misc 插件依赖。
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

    _factor = Fraction(_used).limit_denominator(4)
    rife_out = rife(
        rgb,
        device_index=0,
        model=MODEL,
        factor_num=_factor.numerator,
        factor_den=_factor.denominator,
        scale=SCALE_USED,
        sc=True,                 # 硬切镜不插帧, 避免转场鬼影
        sc_threshold=None,       # 用上面已置好的 _SceneChangeNext (绕开 misc 插件)
        trt=True,                # TensorRT (pypi wheel 自带运行时, 不依赖系统 CUDA)
        trt_static_shape=False,  # 动态形状: 一套引擎覆盖范围内所有分辨率
        # vsrife 默认 opt/max=1920x1080, 超出的源会因引擎形状不符被拒
        # (mpv 会停用滤镜继续无补帧播放)。显式放宽到 4K, 代价是引擎稍大。
        trt_min_shape=[128, 128],
        trt_opt_shape=[1920, 1080],
        trt_max_shape=[3840, 2160],
        trt_cache_dir=TRT_CACHE,
    )
    out = rife_out.resize.Bicubic(
        format=_out_format.id, matrix_s="709", range_s="limited"
    )

out.set_output()
