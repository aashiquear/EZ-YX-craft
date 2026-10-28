#!/usr/bin/env python3
"""Regenerate the Android launcher icons and the iPad app icon from assets/icon.png.

    python3 mobile/make_icons.py      (needs Pillow)

Android keeps the rounded square with transparent corners; iOS needs a full-bleed, opaque
1024 px square (the system rounds it), so the corners are filled with the icon's wood colour.
"""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "assets" / "icon.png"
ANDROID_RES = ROOT / "mobile" / "android" / "app" / "src" / "main" / "res"
IOS_ICON = ROOT / "mobile" / "ios" / "Assets.xcassets" / "AppIcon.appiconset" / "icon-1024.png"
ANDROID_SIZES = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}


def main() -> None:
    src = Image.open(SRC).convert("RGBA")
    for density, px in ANDROID_SIZES.items():
        out = ANDROID_RES / f"mipmap-{density}" / "ic_launcher.png"
        out.parent.mkdir(parents=True, exist_ok=True)
        src.resize((px, px), Image.LANCZOS).save(out, optimize=True)
    wood = src.getpixel((src.width // 2, src.height - 40))[:3]
    big = src.resize((1024, 1024), Image.LANCZOS)
    flat = Image.new("RGB", big.size, wood)
    flat.paste(big, mask=big.split()[3])
    IOS_ICON.parent.mkdir(parents=True, exist_ok=True)
    flat.save(IOS_ICON, optimize=True)
    print(f"wrote {len(ANDROID_SIZES)} Android icons and {IOS_ICON.relative_to(ROOT)} (corner fill {wood})")


if __name__ == "__main__":
    main()
