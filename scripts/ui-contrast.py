#!/usr/bin/env python3
"""WCAG sRGB token contrast audit, not a claim about rasterized glyph coverage."""
import re
from pathlib import Path

def luminance(value):
    channels = [(value >> shift & 255) / 255 for shift in (16, 8, 0)]
    return sum(weight * (v / 12.92 if v <= .04045 else ((v + .055) / 1.055) ** 2.4)
               for v, weight in zip(channels, (.2126, .7152, .0722)))

def ratio(a, b):
    a, b = sorted((luminance(a), luminance(b)))
    return (b + .05) / (a + .05)

source = Path('crates/rgate-ui/src/theme.rs').read_text()
failures = []
for style, appearance, body in re.findall(r'\(Self::(\w+), Appearance::(\w+)\) => Palette \{(.*?)\n            \}', source, re.S):
    colors = {k: int(v, 16) for k, v in re.findall(r'(\w+): 0x([0-9a-f]+)', body)}
    for key in ('text', 'muted', 'gate', 'wire', 'bus', 'low', 'high', 'unknown', 'float', 'selected_wire', 'module'):
        value = ratio(colors[key], colors['panel'])
        print(f'{style}/{appearance} {key}: {value:.2f}:1 (text target 4.5)')
        if value < 4.5: failures.append(f'{style}/{appearance}/{key}')
for name, text, muted, bg, accent, selected, primary, success in [
    ('Light',0x202c3b,0x526174,0xf4f6f8,0xdde9f9,0x174780,0x245caa,0x18734b),
    ('Dark',0xe4eaf2,0xa8b7ca,0x161c25,0x283f60,0xc3dcff,0x8ab9ff,0x69d5a3),
]:
    for label, fg, surface, target in [('text',text,bg,4.5),('muted',muted,bg,4.5),('selected',selected,accent,4.5),('focus',primary,bg,3),('running',success,bg,3)]:
        value = ratio(fg,surface)
        print(f'Chrome/{name} {label}: {value:.2f}:1 (target {target})')
        if value < target: failures.append(f'Chrome/{name}/{label}')
for name, fg, bg in [('Light input',0x7c8999,0xf4f6f8),('Dark input',0x7486a0,0x161c25),('Light selected border',0x245caa,0xdde9f9),('Dark selected border',0x8ab9ff,0x283f60)]:
    value = ratio(fg,bg)
    print(f'{name}: {value:.2f}:1 (target 3)')
    if value < 3: failures.append(name)
print('FAIL: '+', '.join(failures) if failures else 'All audited pairs pass.')
raise SystemExit(bool(failures))
