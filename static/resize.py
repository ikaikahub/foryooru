from PIL import Image

sizes = [128, 192, 512]

sources = {
    "dorito": "dorito.webp",
    "dorito_circle": "dorito circle.webp",
}

for name, path in sources.items():
    img = Image.open(path)
    for size in sizes:
        resized = img.resize((size, size), Image.Resampling.LANCZOS)
        resized.save(f"{name}{size}.png")
