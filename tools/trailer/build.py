"""Turn the frames recorded by the render_trailer test into the README media.

    cargo test --release render_trailer -- --ignored --nocapture
    python tools/trailer/build.py

Writes docs/preview.gif (a silent loop for the top of the README), docs/trailer.mp4
(with the app's own sound) and docs/trailer.png (the picture that links to the video).
Needs the imageio-ffmpeg and Pillow packages.
"""
import os
import subprocess
import wave

import imageio_ffmpeg
from PIL import Image, ImageDraw

FFMPEG = imageio_ffmpeg.get_ffmpeg_exe()
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "target", "trailer")
DOCS = os.path.join(ROOT, "docs")
FPS = "30"


def ffmpeg(*args):
    subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-y", *args], check=True)


def size_mb(path):
    return os.path.getsize(path) / 1048576.0


# The preview: every second frame, 900 pixels wide, one palette for the whole loop.
preview = os.path.join(DOCS, "preview.gif")
ffmpeg(
    "-framerate", FPS, "-i", os.path.join(SRC, "preview", "%05d.png"),
    "-vf", "fps=15,scale=900:-1:flags=lanczos,split[a][b];"
           "[a]palettegen=max_colors=160:stats_mode=diff[p];"
           "[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle",
    "-loop", "0", preview,
)
print("preview.gif  %.1f MB" % size_mb(preview))

# The trailer: H.264 video with the engine's sound, faded out under the end card.
audio = os.path.join(SRC, "audio.wav")
with wave.open(audio, "rb") as w:
    seconds = w.getnframes() / w.getframerate()
trailer = os.path.join(DOCS, "trailer.mp4")
ffmpeg(
    "-framerate", FPS, "-i", os.path.join(SRC, "frames", "%05d.png"), "-i", audio,
    "-vf", "scale=1280:-2:flags=lanczos,format=yuv420p",
    "-c:v", "libx264", "-preset", "slow", "-crf", "20",
    "-af", "afade=t=in:d=0.15,afade=t=out:st=%.2f:d=2.4" % (seconds - 2.4),
    "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", "-shortest", trailer,
)
print("trailer.mp4  %.1f MB, %.1f s" % (size_mb(trailer), seconds))

# The poster: a frame of the app playing, with a play button on it.
frame = Image.open(os.path.join(SRC, "frames", "00120.png")).convert("RGBA")
frame = frame.resize((1280, round(frame.height * 1280 / frame.width)), Image.LANCZOS)
layer = Image.new("RGBA", frame.size, (0, 0, 0, 0))
draw = ImageDraw.Draw(layer)
cx, cy, r = frame.width // 2, frame.height // 2 - 30, 74
draw.rectangle([0, 0, frame.width, frame.height], fill=(8, 9, 12, 110))
draw.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(94, 234, 212, 240))
draw.polygon([(cx - 22, cy - 36), (cx - 22, cy + 36), (cx + 40, cy)], fill=(12, 16, 18, 255))
poster = os.path.join(DOCS, "trailer.png")
Image.alpha_composite(frame, layer).convert("RGB").save(poster, optimize=True)
print("trailer.png  %.1f MB" % size_mb(poster))
