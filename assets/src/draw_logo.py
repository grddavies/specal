import os
from math import tan, radians


# writes into assets/
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")

INK, PINK, BLUE, PORC, PALE = "#0D1821", "#F786AA", "#7161EF", "#F0F4EF", "#FBCADA"
COLS, W_, PITCH = 9, 20, 28
X0 = (320 - (COLS * PITCH - 8)) / 2
# Line angle from vertical
K = tan(radians(8))


def lx(y):
    # x of the diagonal at height y, passes through the centre
    return 160 + K * (160 - y)


G = 6  # half-gap either side of the line

DEFS = f'''  <defs>
    <clipPath id="time"><polygon points="0,0 {lx(0) - G:.1f},0 {lx(320) - G:.1f},320 0,320"/></clipPath>
    <clipPath id="freq"><polygon points="{lx(0) + G:.1f},0 320,0 320,320 {lx(320) + G:.1f},320"/></clipPath>
  </defs>
'''

LINE = f'  <line x1="{lx(28):.1f}" y1="28" x2="{lx(292):.1f}" y2="292" stroke="{PORC}" stroke-width="4" stroke-linecap="round" opacity="0.9"/>\n'


# spectrogram colour by y position
def tone(y):
    return PORC if y < 110 else PALE if y < 160 else PINK if y < 215 else BLUE


H = [2, 4, 6, 4, 8, 6, 8, 4, 2]
bars, cells = "", ""
for i, n in enumerate(H):
    x = X0 + i * PITCH
    h = n * PITCH - 8
    bars += f'    <rect x="{x:g}" y="{160 - h / 2:g}" width="{W_}" height="{h}" rx="5" fill="{PORC}"/>\n'
    ys = {160 - n * PITCH / 2 + k * PITCH + 4 for k in range(n)} | {220, 248}
    for y in sorted(ys):
        cells += f'    <rect x="{x:g}" y="{y:g}" width="{W_}" height="{W_}" rx="5" fill="{tone(y + 10)}"/>\n'
BODY = (
    f'  <rect width="320" height="320" rx="72" fill="{INK}"/>\n  <g clip-path="url(#time)">\n{bars}  </g>\n  <g clip-path="url(#freq)">\n{cells}  </g>\n'
    + LINE
)

svg = (
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 320" role="img" aria-label="Specal">\n'
    + DEFS
    + BODY
    + "</svg>\n"
)

# macOS icon: 824px body centred on a 1024px canvas, so ~100px transparent margin
# (scaled here to the 320 unit art), plus a soft shadow
CANVAS = 320 * 1024 / 824
PAD = (CANVAS - 320) / 2
SHADOW = f'''  <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%">
    <feDropShadow dx="0" dy="{CANVAS * 12 / 1024:.1f}" stdDeviation="{CANVAS * 14 / 1024:.1f}" flood-color="#000" flood-opacity="0.3"/>
  </filter>
'''
shadow_rect = (
    f'  <rect width="320" height="320" rx="72" fill="{INK}" filter="url(#shadow)"/>\n'
)
icon = (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {CANVAS:.2f} {CANVAS:.2f}" role="img" aria-label="Specal">\n'
    + DEFS.replace("  </defs>\n", SHADOW + "  </defs>\n")
    + f'  <g transform="translate({PAD:.2f} {PAD:.2f})">\n'
    + shadow_rect
    + BODY
    + "  </g>\n</svg>\n"
)

open(OUT + "/logo.svg", "w").write(svg)
open(OUT + "/icon-macos.svg", "w").write(icon)
