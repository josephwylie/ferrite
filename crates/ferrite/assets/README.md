# Ferrite application icon

`app-icon.svg` is the editable source artwork. `app-icon.png` is the supplied
1254px transparent raster used to generate the macOS icon set, and
`app-icon.ico` is its Windows build resource.

After changing the artwork, export a transparent square PNG at 1254px and
regenerate the multi-resolution Windows resource with Pillow:

```python
from PIL import Image

image = Image.open("app-icon.png").convert("RGBA")
image.save(
    "app-icon.ico",
    format="ICO",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
```

The macOS installer generates `Ferrite.icns` from `app-icon.png` while
assembling the application bundle.
