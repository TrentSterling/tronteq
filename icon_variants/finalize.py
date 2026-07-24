"""Bake variant 11 (bars over face) as the official TrontEQ icon."""
import shutil
from PIL import Image

SRC = r"C:\trontstack\tronteq\icon_variants\11_bars_over_face.png"
ASSETS = r"C:\trontstack\tronteq\gui\assets"
BAK = r"C:\trontstack\tronteq\icon_variants"

shutil.copy(f"{ASSETS}\\icon.png", f"{BAK}\\original_icon.png")
shutil.copy(f"{ASSETS}\\icon.ico", f"{BAK}\\original_icon.ico")

img = Image.open(SRC)
img.save(f"{ASSETS}\\icon.png")
img.save(f"{ASSETS}\\icon.ico",
         sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("done:", img.size, img.mode)
