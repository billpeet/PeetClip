"""Generate the app's original vector-style icon at Windows icon sizes."""
from pathlib import Path
from PIL import Image, ImageDraw

scale = 4
image = Image.new("RGBA", (256 * scale, 256 * scale))
draw = ImageDraw.Draw(image)

def rounded(box, radius, fill):
    draw.rounded_rectangle(tuple(v * scale for v in box), radius * scale, fill)

rounded((8, 8, 248, 248), 54, "#2563EB")
rounded((66, 51, 190, 215), 18, "#FFFFFF")
rounded((78, 63, 178, 203), 8, "#2563EB")
rounded((97, 39, 159, 78), 12, "#FFFFFF")
rounded((94, 106, 162, 119), 6, "#FFFFFF")
rounded((94, 136, 162, 149), 6, "#FFFFFF")
rounded((94, 166, 143, 179), 6, "#FFFFFF")
image = image.resize((256, 256), Image.Resampling.LANCZOS)
destination = Path(__file__).resolve().parents[1] / "assets" / "clipboard.ico"
image.save(destination, sizes=[(n, n) for n in (16, 20, 24, 32, 40, 48, 64, 128, 256)])
