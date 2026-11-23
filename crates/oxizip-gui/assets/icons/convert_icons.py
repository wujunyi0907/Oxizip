"""Generate Oxizip icons from SVG (Edge headless, transparent RGBA).

Layout:
  svg/         base sources (oxizip brand; small corner badge)
  svg-badge/   format SVGs with enlarged extension labels (48/256)
  <name>.ico   multi-size ICO at icons/ root (256/48/32/16 embedded)

ICO layout note (clean-room observation):
  - DefaultIcon points at a multi-size .ico (no IconHandler).
  - 16/32 frames are hand-tuned: large light glyphs on a solid tile,
    not a downscaled 256px artwork. Explorer list/small views use 16px.
  - We mirror that by painting crisp 16/32 frames with PIL; 48/256 come from SVG.
  - ICO container (7-entry layout): 7 BMP-DIB entries —
    48/32/16 @ 8-bit, then 256/48/32/16 @ 32-bit (height doubled for AND mask).
    No PNG-in-ICO. We match that layout.
"""

from __future__ import annotations

import base64
import io
import os
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent
SVG_DIR = ROOT / "svg"
SVG_BADGE_DIR = ROOT / "svg-badge"

FORMATS = (
    "oxizip",
    "7z",
    "zip",
    "zipx",
    "rar",
    "tar",
    "tgz",
    "tbz",
    "tbz2",
    "gz",
    "bz2",
    "xz",
    "zpaq",
    "egg",
    "alz",
    "cab",
    "iso",
    "lha",
    "lzh",
)
# Largest-first in the ICO directory (many viewers show entry 0).
SIZES = (256, 48, 32, 16)
CRISP_SIZES = (16, 32)
RENDER_SIZE = 256

BG = (9, 25, 48, 255)
ACCENT = (65, 160, 205, 255)
ACCENT_HI = (36, 220, 207, 255)
LABEL = (240, 248, 252, 255)


def _glyph_body() -> str:
    return """<defs><linearGradient id="box" x2="0" y2="1"><stop stop-color="#1c5b8b"/><stop offset="1" stop-color="#10355e"/></linearGradient></defs>
<rect x="2" y="2" width="60" height="60" rx="15" fill="#091930"/>
<path d="M13 19h13l4 4h19l2 24H13z" fill="url(#box)" stroke="#41a0cd" stroke-width="1.3"/>
<path d="M15 26h33v18H15z" fill="#0f2843"/><path d="M27 26h7v18h-7z" fill="#24dccf"/>
<path d="M29 28h3v2h-3zm0 5h3v2h-3zm0 5h3v2h-3z" fill="#acfff1"/>"""


def _badge_fragment(label: str) -> str:
    n = len(label)
    if n <= 2:
        font = 14
    elif n == 3:
        font = 12
    elif n == 4:
        font = 10
    else:
        font = 8
    return f"""{_glyph_body_compact()}
<rect x="6" y="38" width="52" height="20" rx="4" fill="#f0f8fc"/>
<text x="32" y="52.5" text-anchor="middle" font-family="Segoe UI, sans-serif" font-size="{font}" font-weight="700" fill="#13243b">{label}</text>"""


def _glyph_body_compact() -> str:
    return """<defs><linearGradient id="box" x2="0" y2="1"><stop stop-color="#1c5b8b"/><stop offset="1" stop-color="#10355e"/></linearGradient></defs>
<rect x="2" y="2" width="60" height="60" rx="15" fill="#091930"/>
<path d="M16 10h11l3.5 3.5h16l1.5 18H16z" fill="url(#box)" stroke="#41a0cd" stroke-width="1.2"/>
<path d="M18 16h28v12H18z" fill="#0f2843"/><path d="M28 16h6v12h-6z" fill="#24dccf"/>
<path d="M29.5 17.5h3v1.6h-3zm0 3.5h3v1.6h-3zm0 3.5h3v1.6h-3z" fill="#acfff1"/>"""


def _wrap_svg(inner: str) -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" '
        'viewBox="0 0 64 64">\n'
        f"{inner}\n"
        "</svg>\n"
    )


def recreate_svgs() -> None:
    SVG_DIR.mkdir(parents=True, exist_ok=True)
    SVG_BADGE_DIR.mkdir(parents=True, exist_ok=True)
    (SVG_DIR / "oxizip.svg").write_text(_wrap_svg(_glyph_body()), encoding="utf-8")
    for name in FORMATS:
        if name == "oxizip":
            continue
        label = name.upper()
        small = (
            f"{_glyph_body()}"
            f'<rect x="36" y="39" width="24" height="15" rx="4" fill="#f4ae4d"/>'
            f'<text x="48" y="49.5" text-anchor="middle" font-family="Segoe UI, sans-serif" '
            f'font-size="7" font-weight="700" fill="#13243b">{label}</text>'
        )
        (SVG_DIR / f"{name}.svg").write_text(_wrap_svg(small), encoding="utf-8")
        (SVG_BADGE_DIR / f"{name}.svg").write_text(
            _wrap_svg(_badge_fragment(label)), encoding="utf-8"
        )


def find_edge() -> str:
    paths = (
        os.environ.get("EDGE_PATH"),
        shutil.which("msedge"),
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    )
    for path in paths:
        if path and Path(path).is_file():
            return path
    raise FileNotFoundError("Microsoft Edge not found; set EDGE_PATH to msedge.exe")


def render_svg_to_rgba(svg_path: Path, edge: str, size: int = RENDER_SIZE) -> Image.Image:
    encoded = base64.b64encode(svg_path.read_bytes()).decode("ascii")
    with tempfile.TemporaryDirectory(prefix="oxizip-icon-") as tmp:
        html = Path(tmp) / "icon.html"
        png = Path(tmp) / "icon.png"
        html.write_text(
            "<!doctype html><meta charset=utf-8><style>"
            f"html,body{{margin:0;width:{size}px;height:{size}px;overflow:hidden;"
            "background:transparent}}"
            f"img{{display:block;width:{size}px;height:{size}px}}"
            "</style>"
            f"<img src='data:image/svg+xml;base64,{encoded}'>",
            encoding="utf-8",
        )
        subprocess.run(
            [
                edge,
                "--headless=new",
                "--disable-gpu",
                "--hide-scrollbars",
                "--no-first-run",
                "--default-background-color=00000000",
                f"--window-size={size},{size}",
                f"--screenshot={png}",
                html.as_uri(),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        with Image.open(png) as rendered:
            image = rendered.convert("RGBA")
            if image.size != (size, size):
                image = image.resize((size, size), Image.Resampling.LANCZOS)
            return image.copy()


# At 16px, long extensions cannot stay legible — shorten for the label only.
_LABEL_16: dict[str, str] = {
    "TBZ2": "TB2",
    "ZIPX": "ZPX",
    "ZPAQ": "ZPQ",
}


def _label_for_size(size: int, label: str) -> str:
    if size <= 16:
        return _LABEL_16.get(label.upper(), label.upper())
    return label.upper()


def _load_label_font(size: int, label: str) -> ImageFont.ImageFont:
    n = max(1, len(label))
    if size <= 16:
        # Prefer slightly larger glyphs; width check below will shrink if needed.
        target = 11 if n <= 2 else (9 if n == 3 else 8)
    else:
        target = max(8, int(size * (0.55 if n <= 2 else 0.48 if n == 3 else 0.40)))
    candidates = [
        # Tahoma Bold / Verdana Bold read clearer than Segoe at tiny sizes.
        r"C:\Windows\Fonts\tahomabd.ttf",
        r"C:\Windows\Fonts\verdanab.ttf",
        r"C:\Windows\Fonts\segoeuib.ttf",
        r"C:\Windows\Fonts\arialbd.ttf",
        r"C:\Windows\Fonts\arial.ttf",
    ]
    path = next((p for p in candidates if Path(p).is_file()), None)
    if path is None:
        return ImageFont.load_default()
    min_px = 6 if size <= 16 else 5
    for px in range(target, min_px - 1, -1):
        font = ImageFont.truetype(path, px)
        bbox = font.getbbox(label)
        max_w = size - (2 if size <= 16 else 2)
        if bbox[2] - bbox[0] <= max_w:
            return font
    return ImageFont.truetype(path, min_px)


def _draw_tiny_mark(draw: ImageDraw.ImageDraw, size: int, pad: int) -> None:
    """Compact archive mark — leaves most of the 16px tile for the label."""
    if size <= 16:
        # 3px-tall zipper hint only (text dominates at 16).
        y0 = pad
        draw.rectangle([pad + 3, y0, size - 2 - pad, y0 + 2], fill=ACCENT_HI)
        mid = size // 2
        draw.point((mid, y0 + 1), fill=BG)
        draw.point((mid - 2, y0 + 1), fill=BG)
        draw.point((mid + 2, y0 + 1), fill=BG)
        return
    top = pad + 2
    mid = size // 2
    mark_h = max(3, size // 2 - top - 1)
    mark_w = max(6, size // 2)
    x0 = (size - mark_w) // 2
    draw.rectangle([x0, top, x0 + mark_w, top + mark_h], outline=ACCENT, width=1)
    bar = max(1, mark_h // 3)
    draw.rectangle([x0, top, x0 + mark_w, top + bar], fill=ACCENT_HI)
    zx = mid - max(1, size // 16)
    draw.rectangle([zx, top + bar, zx + max(1, size // 8), top + mark_h], fill=ACCENT_HI)


def _harden_glyph(layer: Image.Image, ink: tuple[int, int, int, int]) -> Image.Image:
    """Keep strong AA coverage as solid ink; drop weak fringe (sharper at 16px)."""
    out = Image.new("RGBA", layer.size, (0, 0, 0, 0))
    sp, op = layer.load(), out.load()
    w, h = layer.size
    for y in range(h):
        for x in range(w):
            _r, _g, _b, a = sp[x, y]
            if a >= 140:
                op[x, y] = ink
    return out


def draw_crisp_format_icon(size: int, label: str) -> Image.Image:
    label = _label_for_size(size, label)
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    pad = 1 if size <= 16 else max(1, size // 16)
    radius = max(2, size // 5)
    draw.rounded_rectangle(
        [pad, pad, size - 1 - pad, size - 1 - pad],
        radius=radius,
        fill=BG,
    )
    if size >= 16:
        _draw_tiny_mark(draw, size, pad)

    font = _load_label_font(size, label)
    bbox = draw.textbbox((0, 0), label, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    x = (size - tw) // 2 - bbox[0]
    if size <= 16:
        # Text owns most of the tile under the 3px mark.
        band_top = pad + 4
        bottom_pad = 1
    else:
        band_top = size // 2
        bottom_pad = 4
    y = band_top + (size - bottom_pad - band_top - th) // 2 - bbox[1]
    ink_bottom = y + bbox[3]
    max_ink_bottom = size - 1 - bottom_pad
    if ink_bottom > max_ink_bottom:
        y -= ink_bottom - max_ink_bottom

    if size <= 16:
        # No stroke at 16px (merges glyphs). Harden AA into solid white ink.
        layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        ImageDraw.Draw(layer).text((x, y), label, font=font, fill=LABEL)
        img.alpha_composite(_harden_glyph(layer, LABEL))
    else:
        draw.text((x, y), label, font=font, fill=LABEL)
    return img


def _row_stride(width: int, bpp: int) -> int:
    return ((width * bpp + 31) // 32) * 4


def _and_mask_from_alpha(image: Image.Image, threshold: int = 16) -> bytes:
    """1-bpp AND mask (1 = transparent), bottom-up, 32-bit row padded.

    Important: bytearray slices are copies — write into ``out`` by index.
    """
    w, h = image.size
    alpha = image.getchannel("A")
    stride = _row_stride(w, 1)
    out = bytearray(stride * h)
    for y in range(h):
        src_y = h - 1 - y
        row_off = y * stride
        for x in range(w):
            if alpha.getpixel((x, src_y)) < threshold:
                out[row_off + (x >> 3)] |= 0x80 >> (x & 7)
    return bytes(out)


def _dib_header(
    width: int,
    height: int,
    bit_count: int,
    size_image: int,
    clr_used: int = 0,
) -> bytes:
    # ICO DIBs store height = image + AND mask.
    return struct.pack(
        "<IiiHHIIiiII",
        40,
        width,
        height * 2,
        1,
        bit_count,
        0,
        size_image,
        2835,  # ~72 DPI, same ballpark as common shell ICOs (0x0b13)
        2835,
        clr_used,
        clr_used,
    )


def _rgba_to_dib32(image: Image.Image) -> bytes:
    rgba = image.convert("RGBA")
    w, h = rgba.size
    pixels = rgba.load()
    stride = _row_stride(w, 32)
    xor = bytearray(stride * h)
    for y in range(h):
        src_y = h - 1 - y
        row_off = y * stride
        for x in range(w):
            r, g, b, a = pixels[x, src_y]
            i = row_off + x * 4
            xor[i : i + 4] = bytes((b, g, r, a))
    and_mask = _and_mask_from_alpha(rgba)
    header = _dib_header(w, h, 32, len(xor), 0)
    return header + bytes(xor) + and_mask


def _rgba_to_dib8(image: Image.Image) -> bytes:
    """8-bit indexed XOR + 256-color BGRA palette + AND mask."""
    rgba = image.convert("RGBA")
    w, h = rgba.size
    # Force near-transparent pixels fully transparent so AND mask is clean.
    flat = rgba.copy()
    px = flat.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a < 16:
                px[x, y] = (0, 0, 0, 0)
            else:
                px[x, y] = (r, g, b, 255)

    # Adaptive palette; keep room for a transparent index 0.
    quantized = flat.convert("RGB").quantize(colors=255, method=Image.Quantize.MEDIANCUT)
    raw_pal = quantized.getpalette() or []
    n_colors = max(1, min(255, len(raw_pal) // 3))
    # Map: index 0 = transparent; image indices shifted +1.
    pal = bytearray(256 * 4)
    for i in range(n_colors):
        r = raw_pal[i * 3]
        g = raw_pal[i * 3 + 1]
        b = raw_pal[i * 3 + 2]
        off = (i + 1) * 4
        pal[off : off + 4] = bytes((b, g, r, 0))

    src_idx = quantized.load()
    alpha = rgba.getchannel("A")
    stride = _row_stride(w, 8)
    xor = bytearray(stride * h)
    for y in range(h):
        src_y = h - 1 - y
        row_off = y * stride
        for x in range(w):
            if alpha.getpixel((x, src_y)) < 16:
                xor[row_off + x] = 0
            else:
                idx = int(src_idx[x, src_y])
                xor[row_off + x] = min(idx, n_colors - 1) + 1

    and_mask = _and_mask_from_alpha(rgba)
    header = _dib_header(w, h, 8, len(xor), 256)
    return header + bytes(pal) + bytes(xor) + and_mask


def write_multi_size_ico(path: Path, frames: list[tuple[int, Image.Image]]) -> None:
    """Write multi-size ICO: 8-bit 48/32/16 then 32-bit 256/48/32/16 (BMP-DIB)."""
    by_size = {size: image.convert("RGBA") for size, image in frames}
    # Prefer the 7-entry size set; extra sizes (e.g. 24) still get both depths.
    preferred_8 = [s for s in (48, 32, 16) if s in by_size]
    preferred_8 += sorted((s for s in by_size if s not in preferred_8 and s < 256), reverse=True)
    preferred_32 = [s for s in (256, 48, 32, 16) if s in by_size]
    preferred_32 += sorted((s for s in by_size if s not in preferred_32), reverse=True)

    entries: list[tuple[int, int, bytes]] = []
    for size in preferred_8:
        entries.append((size, 8, _rgba_to_dib8(by_size[size])))
    for size in preferred_32:
        entries.append((size, 32, _rgba_to_dib32(by_size[size])))

    if not entries:
        raise RuntimeError("ICO has no frames")

    count = len(entries)
    offset = 6 + 16 * count
    header = struct.pack("<HHH", 0, 1, count)
    directory = bytearray()
    blobs = bytearray()
    for size, bit_count, blob in entries:
        w = 0 if size >= 256 else size
        h = 0 if size >= 256 else size
        directory += struct.pack(
            "<BBBBHHII", w, h, 0, 0, 1, bit_count, len(blob), offset
        )
        blobs += blob
        offset += len(blob)

    data = header + directory + blobs
    path.write_bytes(data)
    got = struct.unpack_from("<H", data, 4)[0]
    if got != count:
        raise RuntimeError(f"ICO header count mismatch: wrote {count}, header {got}")


def write_outputs(name: str, frames: list[tuple[int, Image.Image]]) -> None:
    # Remove legacy per-format directories from earlier layout.
    legacy_dir = ROOT / name
    if legacy_dir.is_dir():
        shutil.rmtree(legacy_dir, ignore_errors=True)

    ico_path = ROOT / f"{name}.ico"
    tmp_ico = ROOT / f"{name}.ico.tmp"
    write_multi_size_ico(tmp_ico, frames)
    tmp_ico.replace(ico_path)
    size_list = ", ".join(f"{s}x{s}" for s, _ in frames)
    print(
        f"created {name}.ico  rgba_frames={len(frames)} [{size_list}]  "
        f"bytes={ico_path.stat().st_size}"
    )


def convert_format(name: str, edge: str) -> None:
    label = name.upper()
    svg = SVG_BADGE_DIR / f"{name}.svg"
    master = render_svg_to_rgba(svg, edge, RENDER_SIZE)
    frames: list[tuple[int, Image.Image]] = []
    for size in SIZES:
        if size in CRISP_SIZES:
            frames.append((size, draw_crisp_format_icon(size, label)))
        else:
            frames.append((size, master.resize((size, size), Image.Resampling.LANCZOS)))
    write_outputs(name, frames)


def convert_brand(edge: str) -> None:
    master = render_svg_to_rgba(SVG_DIR / "oxizip.svg", edge, RENDER_SIZE)
    frames = [
        (size, master.resize((size, size), Image.Resampling.LANCZOS)) for size in SIZES
    ]
    write_outputs("oxizip", frames)


def _hicon_to_image(hicon: int, size: int) -> Image.Image:
    """Draw a Win32 HICON into an RGBA PIL image."""
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.windll.user32
    gdi32 = ctypes.windll.gdi32

    class BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [
            ("biSize", wintypes.DWORD),
            ("biWidth", ctypes.c_long),
            ("biHeight", ctypes.c_long),
            ("biPlanes", wintypes.WORD),
            ("biBitCount", wintypes.WORD),
            ("biCompression", wintypes.DWORD),
            ("biSizeImage", wintypes.DWORD),
            ("biXPelsPerMeter", ctypes.c_long),
            ("biYPelsPerMeter", ctypes.c_long),
            ("biClrUsed", wintypes.DWORD),
            ("biClrImportant", wintypes.DWORD),
        ]

    class BITMAPINFO(ctypes.Structure):
        _fields_ = [("bmiHeader", BITMAPINFOHEADER), ("bmiColors", wintypes.DWORD * 3)]

    hdc = user32.GetDC(0)
    hdc_mem = gdi32.CreateCompatibleDC(hdc)
    bmi = BITMAPINFO()
    bmi.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    bmi.bmiHeader.biWidth = size
    bmi.bmiHeader.biHeight = -size
    bmi.bmiHeader.biPlanes = 1
    bmi.bmiHeader.biBitCount = 32
    bits = ctypes.c_void_p()
    hbmp = gdi32.CreateDIBSection(hdc, ctypes.byref(bmi), 0, ctypes.byref(bits), None, 0)
    old = gdi32.SelectObject(hdc_mem, hbmp)
    ctypes.memset(bits, 0, size * size * 4)
    user32.DrawIconEx(hdc_mem, 0, 0, hicon, size, size, 0, None, 0x0003)
    buf = ctypes.string_at(bits, size * size * 4)
    img = Image.frombuffer("RGBA", (size, size), buf, "raw", "BGRA", 0, 1).copy()
    gdi32.SelectObject(hdc_mem, old)
    gdi32.DeleteObject(hbmp)
    gdi32.DeleteDC(hdc_mem)
    user32.ReleaseDC(0, hdc)
    return img


def extract_uac_shield(size: int) -> Image.Image:
    """Windows stock UAC shield (imageres SIID_SHIELD) — system asset, not third-party art."""
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.windll.user32
    shell32 = ctypes.windll.shell32

    class SHSTOCKICONINFO(ctypes.Structure):
        _fields_ = [
            ("cbSize", wintypes.DWORD),
            ("hIcon", wintypes.HICON),
            ("iSysImageIndex", ctypes.c_int),
            ("iIcon", ctypes.c_int),
            ("szPath", wintypes.WCHAR * 260),
        ]

    info = SHSTOCKICONINFO()
    info.cbSize = ctypes.sizeof(info)
    hr = shell32.SHGetStockIconInfo(77, 0x100, ctypes.byref(info))  # SIID_SHIELD
    if hr != 0:
        raise OSError(f"SHGetStockIconInfo failed: {hr}")
    try:
        hicon = wintypes.HICON()
        icon_id = wintypes.UINT()
        n = user32.PrivateExtractIconsW(
            info.szPath,
            info.iIcon,
            size,
            size,
            ctypes.byref(hicon),
            ctypes.byref(icon_id),
            1,
            0,
        )
        if n < 1 or not hicon.value:
            # Fall back to the stock handle (often 32/16).
            return _hicon_to_image(info.hIcon, size)
        try:
            return _hicon_to_image(hicon.value, size)
        finally:
            user32.DestroyIcon(hicon)
    finally:
        user32.DestroyIcon(info.hIcon)


def _setup_glass_svg() -> str:
    """Molded acrylic open tray (clean-room SVG).

    Reference palette: milky face ~lum 210 (e.g. 168,210,255); top rim near-solid
    white band; FR vertical rim ~x0.63. Faces are filled material, not neon wire.
    """
    return """<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <defs>
    <linearGradient id="milkFront" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#f2f8ff" stop-opacity="0.42"/>
      <stop offset="40%" stop-color="#dcecff" stop-opacity="0.38"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0.72"/>
    </linearGradient>
    <linearGradient id="milkSide" x1="0" y1="0" x2="1" y2="0.6">
      <stop offset="0%" stop-color="#eaf4ff" stop-opacity="0.35"/>
      <stop offset="100%" stop-color="#f8fcff" stop-opacity="0.58"/>
    </linearGradient>
    <linearGradient id="milkFloor" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.55"/>
      <stop offset="100%" stop-color="#e8eef4" stop-opacity="0.82"/>
    </linearGradient>
    <linearGradient id="lip" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="1"/>
      <stop offset="55%" stop-color="#f0f9ff" stop-opacity="0.95"/>
      <stop offset="100%" stop-color="#c5e0f8" stop-opacity="0.55"/>
    </linearGradient>
    <linearGradient id="rimV" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.2"/>
      <stop offset="45%" stop-color="#ffffff" stop-opacity="1"/>
      <stop offset="100%" stop-color="#d0e8fa" stop-opacity="0.35"/>
    </linearGradient>
    <filter id="soft" x="-15%" y="-15%" width="130%" height="130%">
      <feGaussianBlur stdDeviation="0.8"/>
    </filter>
    <filter id="softer" x="-25%" y="-25%" width="150%" height="150%">
      <feGaussianBlur stdDeviation="1.6"/>
    </filter>
  </defs>
  <!-- molded body fills (substance, not wireframe) -->
  <polygon points="30,188 155,188 212,222 55,222" fill="url(#milkFloor)" filter="url(#soft)"/>
  <polygon points="161,142 218,114 218,222 161,236" fill="url(#milkSide)" filter="url(#soft)"/>
  <polygon points="24,142 161,142 161,236 24,236" fill="url(#milkFront)" filter="url(#soft)"/>
  <!-- wide top lip / bevel (solid white band across opening) -->
  <rect x="28" y="138" width="130" height="18" rx="6" fill="url(#lip)" filter="url(#softer)"/>
  <rect x="34" y="142" width="118" height="8" rx="3" fill="#ffffff" opacity="0.95"/>
  <!-- front-right vertical rim as thick filled capsule (~x161) -->
  <rect x="154" y="142" width="16" height="96" rx="5" fill="url(#rimV)" filter="url(#soft)"/>
  <rect x="157" y="146" width="8" height="88" rx="3" fill="#ffffff" opacity="0.92"/>
  <!-- top-right perspective lip -->
  <path d="M161,146 L216,118 L220,126 L165,154 Z" fill="#ffffff" opacity="0.88" filter="url(#soft)"/>
  <!-- left / bottom soft rims -->
  <rect x="22" y="146" width="10" height="88" rx="4" fill="#eef6ff" opacity="0.75" filter="url(#soft)"/>
  <rect x="28" y="228" width="130" height="10" rx="4" fill="#c8d8e8" opacity="0.7" filter="url(#soft)"/>
  <path d="M161,232 L216,220 L218,228 L163,240 Z" fill="#c8d8e8" opacity="0.65" filter="url(#soft)"/>
  <!-- download arrow (~ rgb 0,56,159) -->
  <g fill="#00389f">
    <rect x="76" y="168" width="16" height="38" rx="2.5"/>
    <polygon points="84,224 60,190 108,190"/>
  </g>
</svg>
"""


def _brighten_product_tile(brand: Image.Image) -> Image.Image:
    """Bright product squircle with Oxizip glyph on top.

    Glass milky overlays need a luminous blue plate underneath; stock oxizip.ico
    navy turns any frost grey.
    """
    src = brand.convert("RGBA")
    w, h = src.size
    plate = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(plate)
    # Vertical blue gradient plate (~ bright mid-blue).
    for y in range(h):
        t = y / max(1, h - 1)
        r = int(40 + 20 * (1 - t))
        g = int(120 + 40 * (1 - t))
        b = int(220 + 35 * (1 - t))
        d.line([(0, y), (w - 1, y)], fill=(r, g, b, 255))
    mask = Image.new("L", (w, h), 0)
    md = ImageDraw.Draw(mask)
    rad = max(8, w // 5)
    md.rounded_rectangle([0, 0, w - 1, h - 1], radius=rad, fill=255)
    plate.putalpha(mask)

    # Lift glyph accents and screen onto the plate (keep zipper/folder readable).
    glyph = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    sp, gp = src.load(), glyph.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = sp[x, y]
            if a < 20:
                continue
            # Prefer bright accents / midtones; skip near-black navy fill.
            if r + g + b < 90:
                continue
            r2 = min(255, int(r * 0.35 + 180))
            g2 = min(255, int(g * 0.35 + 220))
            b2 = min(255, int(b * 0.25 + 245))
            gp[x, y] = (r2, g2, b2, min(255, int(a * 0.92)))
    return Image.alpha_composite(plate, glyph)


def _render_setup_glass(edge: str, size: int) -> Image.Image:
    """Rasterize the acrylic tray SVG via Edge (soft AA + gradients)."""
    svg_path = ROOT / "svg" / "oxizip-setup-glass.svg"
    svg_path.parent.mkdir(parents=True, exist_ok=True)
    text = _setup_glass_svg()
    if not svg_path.is_file() or svg_path.read_text(encoding="utf-8") != text:
        svg_path.write_text(text, encoding="utf-8")
    master = render_svg_to_rgba(svg_path, edge, 256)
    if size == 256:
        return master
    return master.resize((size, size), Image.Resampling.LANCZOS)


def compose_uac_frame(size: int, brand: Image.Image) -> Image.Image:
    """Full UAC shield with Oxizip badge (shellicons/uac.ico layout)."""
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shield = extract_uac_shield(size)
    if shield.size != (size, size):
        shield = shield.resize((size, size), Image.Resampling.LANCZOS)
    canvas.alpha_composite(shield)
    badge_size = max(8, int(round(size * (0.58 if size <= 16 else 0.52))))
    badge = brand.resize((badge_size, badge_size), Image.Resampling.LANCZOS)
    pad = 0 if size <= 16 else max(1, size // 32)
    x = size - badge_size - pad
    y = size - badge_size - pad
    canvas.alpha_composite(badge, (x, y))
    return canvas


def _trim_alpha(image: Image.Image, threshold: int = 16) -> Image.Image:
    """Crop to opaque content so alignment uses the visible glyph, not padding."""
    rgba = image.convert("RGBA")
    alpha = rgba.getchannel("A")
    bbox = alpha.point(lambda a: 255 if a >= threshold else 0).getbbox()
    if bbox is None:
        return rgba
    return rgba.crop(bbox)


def compose_setup_frame(size: int, brand: Image.Image, edge: str | None = None) -> Image.Image:
    """Setup PE icon: Oxizip brand + UAC shield (visible left edge on midline)."""
    del edge  # unused; kept for call-site compatibility
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # Brand fills most of the tile.
    brand_size = max(10, int(round(size * (0.92 if size >= 32 else 0.88))))
    brand_img = brand.convert("RGBA").resize(
        (brand_size, brand_size), Image.Resampling.LANCZOS
    )
    bx = (size - brand_size) // 2
    by = max(0, (size - brand_size) // 2 - max(0, size // 32))
    canvas.alpha_composite(brand_img, (bx, by))

    # Shield: visible left edge on vertical midline; bottom-right of brand.
    # Slightly larger, nudged up from the tile bottom.
    if size <= 16:
        shield_size = max(8, int(size * 0.56))
    elif size <= 32:
        shield_size = max(16, int(size * 0.54))
    elif size <= 48:
        shield_size = max(22, int(size * 0.52))
    else:
        shield_size = max(32, int(size * 0.48))
    shield = _trim_alpha(extract_uac_shield(max(48, shield_size * 3)))
    # Keep aspect after trim, fit into shield_size box.
    sw, sh = shield.size
    scale = shield_size / max(sw, sh)
    nw = max(1, int(round(sw * scale)))
    nh = max(1, int(round(sh * scale)))
    shield = shield.resize((nw, nh), Image.Resampling.LANCZOS)
    sx = size // 2  # visible left edge on midline
    # Near bottom; small inset only (~2–3% of tile).
    sy = size - nh - max(1, int(round(size * 0.025)))
    if sx + nw > size:
        sx = size - nw
    sy = max(0, sy)
    canvas.alpha_composite(shield, (sx, sy))
    return canvas


def _load_brand_master() -> Image.Image:
    brand_path = ROOT / "oxizip.ico"
    if not brand_path.is_file():
        raise FileNotFoundError(brand_path)
    with Image.open(brand_path) as ico:
        best = max(ico.ico.sizes())
        return ico.ico.getimage(best).convert("RGBA")


def convert_uac_icon() -> None:
    """oxizip-uac.ico: full shield + brand badge (for elevated shell verbs later)."""
    brand = _load_brand_master()
    frames = [(s, compose_uac_frame(s, brand)) for s in SIZES]
    write_outputs("oxizip-uac", frames)


def convert_setup_icon(edge: str | None = None) -> None:
    """oxizip-setup.ico: installer PE + window icon (app keeps oxizip.ico)."""
    if edge is None:
        edge = find_edge()
    brand = _load_brand_master()
    frames = [(s, compose_setup_frame(s, brand, edge)) for s in SIZES]
    write_outputs("oxizip-setup", frames)


def main() -> None:
    recreate_svgs()
    edge = find_edge()
    convert_brand(edge)
    for name in FORMATS:
        if name == "oxizip":
            continue
        convert_format(name, edge)
    convert_uac_icon()
    convert_setup_icon(edge)


if __name__ == "__main__":
    main()

