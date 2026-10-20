#!/usr/bin/env python3
"""Turns the frames from the `demo_frames` test into the README animation.

    SCREENSHOTS=target/screenshots cargo test demo_frames -- --ignored
    python3 scripts/demo-gif.py target/screenshots/demo docs/screenshots/demo.gif \
        --webp website/static/img/demo.webp

Each frame shows for the time listed next to it in frames.txt. The GIF uses
one palette for every frame (no flicker between frames); the WebP is for the
website, sharper and smaller. Needs Pillow.
"""

import argparse
from pathlib import Path

from PIL import Image


def load(frames_dir: Path, width: int):
    frames = []
    for line in (frames_dir / "frames.txt").read_text().split("\n"):
        if not line.strip():
            continue
        name, ms = line.split()
        image = Image.open(frames_dir / name).convert("RGB")
        height = round(image.height * width / image.width)
        frames.append((image.resize((width, height), Image.LANCZOS), int(ms)))
    return frames


def merge_repeats(frames):
    """Folds identical neighbours into one longer frame."""
    merged = []
    for image, ms in frames:
        if merged and merged[-1][0].tobytes() == image.tobytes():
            merged[-1] = (merged[-1][0], merged[-1][1] + ms)
        else:
            merged.append((image, ms))
    return merged


def shared_palette(images):
    """A 256-colour palette that fits every frame: quantize a strip of all of them."""
    strip = Image.new("RGB", (images[0].width, images[0].height * len(images)))
    for i, image in enumerate(images):
        strip.paste(image, (0, i * images[0].height))
    sample = strip.resize((strip.width // 2, strip.height // 2), Image.NEAREST)
    return sample.quantize(colors=256, method=Image.Quantize.MEDIANCUT)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("frames", type=Path, help="folder with frames.txt and the PNGs")
    parser.add_argument("gif", type=Path)
    parser.add_argument("--width", type=int, default=1240)
    parser.add_argument("--webp", type=Path, help="also write an animated WebP")
    parser.add_argument("--webp-width", type=int, default=1600)
    args = parser.parse_args()

    frames = merge_repeats(load(args.frames, args.width))
    palette = shared_palette([image for image, _ in frames])
    indexed = [image.quantize(palette=palette, dither=Image.Dither.NONE) for image, _ in frames]
    indexed[0].save(
        args.gif,
        save_all=True,
        append_images=indexed[1:],
        duration=[ms for _, ms in frames],
        loop=0,
        optimize=False,
        disposal=1,
    )
    print(f"{args.gif}: {len(frames)} frames, {args.gif.stat().st_size / 1e6:.2f} MB")

    if args.webp:
        frames = merge_repeats(load(args.frames, args.webp_width))
        frames[0][0].save(
            args.webp,
            save_all=True,
            append_images=[image for image, _ in frames[1:]],
            duration=[ms for _, ms in frames],
            loop=0,
            quality=82,
            method=6,
        )
        print(f"{args.webp}: {len(frames)} frames, {args.webp.stat().st_size / 1e6:.2f} MB")


if __name__ == "__main__":
    main()
