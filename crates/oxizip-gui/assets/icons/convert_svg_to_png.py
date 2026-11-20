"""Convert SVGs under svg/ and svg-badge/ to transparent 256x256 PNG (Edge headless).

Outputs next to the script as <stem>.png / <stem>-badge.png (preview only).
"""

from __future__ import annotations

import base64
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent
SVG_DIR = ROOT / "svg"
SVG_BADGE_DIR = ROOT / "svg-badge"
SIZE = 256
SKIP = frozenset({"chevron_down.svg", "chevron_right.svg"})


def find_edge() -> str:
    candidates = (
        os.environ.get("EDGE_PATH"),
        shutil.which("msedge"),
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    )
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return candidate
    raise FileNotFoundError("Microsoft Edge not found; set EDGE_PATH to msedge.exe")


def render(svg_path: Path, edge: str) -> Image.Image:
    svg_data = base64.b64encode(svg_path.read_bytes()).decode("ascii")
    with tempfile.TemporaryDirectory(prefix="oxizip-svg-") as temp_dir:
        temp = Path(temp_dir)
        html = temp / "render.html"
        screenshot = temp / "render.png"
        html.write_text(
            "<!doctype html><meta charset='utf-8'><style>"
            f"html,body{{margin:0;padding:0;width:{SIZE}px;height:{SIZE}px;overflow:hidden;"
            "background:transparent}}"
            f"img{{display:block;width:{SIZE}px;height:{SIZE}px}}"
            "</style>"
            f"<img src='data:image/svg+xml;base64,{svg_data}'>",
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
                f"--window-size={SIZE},{SIZE}",
                f"--screenshot={screenshot}",
                html.as_uri(),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        with Image.open(screenshot) as rendered:
            image = rendered.convert("RGBA")
            if image.size != (SIZE, SIZE):
                image = image.resize((SIZE, SIZE), Image.Resampling.LANCZOS)
            return image.copy()


def main() -> None:
    edge = find_edge()
    for folder, suffix in ((SVG_DIR, ".png"), (SVG_BADGE_DIR, "-badge.png")):
        if not folder.is_dir():
            continue
        for svg_path in sorted(folder.glob("*.svg")):
            if svg_path.name in SKIP:
                continue
            out = ROOT / f"{svg_path.stem}{suffix}"
            render(svg_path, edge).save(out, format="PNG", optimize=True)
            print(f"created {out.name}")


if __name__ == "__main__":
    main()
