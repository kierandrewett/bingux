#!/usr/bin/env python3
"""Lay out captured GTK reference and SSD frames without altering their pixels."""
from pathlib import Path
import sys
from PIL import Image, ImageDraw, ImageFont

root = Path(sys.argv[1])
font = ImageFont.load_default(size=15)
phases = ('Idle', 'Hover', 'Pressed', 'Pointer leaves', 'Focus lost', 'Focus restored')
frames = []
for tick in range(180):
    gtk = Image.open(root / f'gtk-{tick:03}.png').convert('RGB')
    ssd = Image.open(root / f'ssd-{tick:03}.png').convert('RGB')
    canvas = Image.new('RGB', (gtk.width + 180, gtk.height * 2 + 64), '#161619')
    draw = ImageDraw.Draw(canvas)
    draw.text((12, 8), phases[tick // 30], fill='white', font=font)
    draw.text((12, 40), 'Normal GTK app', fill='white', font=font)
    draw.text((12, gtk.height + 52), 'Bingux SSD', fill='white', font=font)
    canvas.paste(gtk, (166, 32))
    canvas.paste(ssd, (166, gtk.height + 44))
    frames.append(canvas)
frames[0].save(root / 'comparison.gif', save_all=True, append_images=frames[1:],
               duration=20, loop=0, disposal=2)
overview = Image.new('RGB', (frames[0].width, frames[0].height * 4), '#161619')
for row, tick in enumerate((29, 59, 89, 149)):
    overview.paste(frames[tick], (0, row * frames[0].height))
overview.save(root / 'comparison.png')
print(root / 'comparison.gif')
print(root / 'comparison.png')
