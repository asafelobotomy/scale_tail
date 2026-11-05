#!/usr/bin/env python3
"""Generates the shield-with-scales icons (SVG), then run rsvg-convert for PNGs.
on = light green (connected), off = dark green (disconnected)."""
import math, os
HERE = os.path.dirname(os.path.abspath(__file__))
SHIELD = "M32 3 L55 11 V30 C55 46 45 56 32 61 C19 56 9 46 9 30 V11 Z"
THEMES = {
    "on":  dict(shield="#4ade80", edge="#14532d", scale="#86efac", line="#15803d"),
    "off": dict(shield="#14532d", edge="#052e16", scale="#166534", line="#052e16"),
}
def svg(t):
    r, circles = 6.5, []
    y, row = 8.0, 0
    while y < 70:                      # top-to-bottom so lower scales overlap upper ones
        x = 9 - (r if row % 2 else 0)
        while x < 60:
            circles.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r}"/>')
            x += r * 2
        y += r * 0.9; row += 1
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<defs><clipPath id="s"><path d="{SHIELD}"/></clipPath></defs>
<path d="{SHIELD}" fill="{t['shield']}"/>
<g clip-path="url(#s)" fill="{t['scale']}" stroke="{t['line']}" stroke-width="1.2">
{chr(10).join(circles)}
</g>
<path d="{SHIELD}" fill="none" stroke="{t['edge']}" stroke-width="3" stroke-linejoin="round"/>
</svg>
'''
for name, t in THEMES.items():
    open(os.path.join(HERE, f"scale_tail_{name}.svg"), "w").write(svg(t))
