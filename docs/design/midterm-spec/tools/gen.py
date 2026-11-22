#!/usr/bin/env python3
"""Generates mockup.html (1:1 recreation of midu's midterm screenshot).
All coordinates are LOGICAL px = screenshot px / 1.5 (see spec.md)."""
import json, html, os
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
T = json.load(open(os.path.join(HERE, 'tokens.json')))
C = T['color']; F = T['font']

W, H = 1837.33, 1033.33
SB = 309.33          # sidebar width incl. 1px border
TAB_H = 38.0
DIV_X = 998.0        # pane divider x
ROW0 = 53.33         # first terminal row top
LH = F['mono_line_height']
CW = F['mono_advance']
LCOL0 = 324.67       # left pane col 0 x
RCOL0 = 1013.33      # right pane col 0 x

out = []
def a(s): out.append(s)
def esc(s): return html.escape(s, quote=False)

def ui_text(x, baseline, text, size, color, weight=None, extra='', anchor='left', width=None):
    # Inter: baseline sits 0.9636em below the top of a line-height:1.2 box
    top = baseline - 0.9636 * size
    if anchor == 'right':
        a(f'<div class="t" style="right:{W - x:.2f}px;top:{top:.2f}px;font-size:{size}px;color:{color};font-weight:{weight or 460};{extra}">{esc(text)}</div>')
    else:
        w = f'width:{width}px;' if width else ''
        a(f'<div class="t" style="left:{x:.2f}px;top:{top:.2f}px;font-size:{size}px;color:{color};font-weight:{weight or 460};{w}{extra}">{esc(text)}</div>')

def rect(x, y, w, h, bg, r=0, extra=''):
    a(f'<div class="r" style="left:{x:.2f}px;top:{y:.2f}px;width:{w:.2f}px;height:{h:.2f}px;background:{bg};border-radius:{r}px;{extra}"></div>')

ICON = json.load(open(os.path.join(HERE, 'iconmeta.json')))
def icon(name, x=None, y=None):
    m = ICON[name]
    x = m['x'] if x is None else x; y = m['y'] if y is None else y
    a(f'<img class="i" src="assets/{name}.png" style="left:{x:.2f}px;top:{y:.2f}px;width:{m["w"]}px;height:{m["h"]}px">')

# ---------------------------------------------------------------- terminal
def term_line(col0, row, segs, cls=''):
    top = ROW0 + (row - 1) * LH
    spans = ''.join(f'<span style="color:{C.get(c, c)}{";font-weight:700" if b else ""}">{esc(t)}</span>' for t, c, b in segs)
    a(f'<div class="m {cls}" style="left:{col0:.2f}px;top:{top:.2f}px">{spans}</div>')

def band(col0, row, c0, c1, color, nrows=1):
    top = ROW0 + (row - 1) * LH
    rect(col0 + c0 * CW, top, (c1 - c0) * CW, LH * nrows, color)

def seg(spec):
    """mini markup: list of (text, colorkey[, bold])"""
    res = []
    for s in spec:
        if isinstance(s, str): res.append((s, 'fg', False))
        else: res.append((s[0], s[1], len(s) > 2 and s[2]))
    return res

LEFT = json.load(open(os.path.join(HERE, 'term_left.json')))
RIGHT = json.load(open(os.path.join(HERE, 'term_right.json')))

def pane(col0, data, cursor_focused):
    for b in data['bands']:
        band(col0, b[0], b[2], b[3], C[b[4]], b[1] - b[0] + 1)
    for r, segs in data['lines'].items():
        term_line(col0, int(r), seg(segs))
    for ov in data.get('overlays', []):
        row, c, text = ov
        top = ROW0 + (row - 1) * LH
        rect(col0 + c * CW, top, len(text) * CW, LH, C['term_overlay_bg'])
        term_line(col0 + c * CW, row, [(text, C['term_fg_bright'], False)])
    # separator rows (Claude Code input box) drawn with box-drawing glyphs
    ncols = data['cols']
    for row in data['sep_rows']:
        term_line(col0, row, [('─' * ncols, 'term_sep', False)])
    # cursor
    crow, ccol = data['cursor']
    top = ROW0 + (crow - 1) * LH
    if cursor_focused:
        rect(col0 + ccol * CW, top + 1.5, 2, LH - 3, C['cursor'])
    else:
        rect(col0 + ccol * CW, top + 1.5, CW - 1, LH - 3, 'transparent', 0, f'border:1px solid {C["cursor"]};box-sizing:border-box')
    # status bar progress (thin line, 15 %)
    srow, s0, s1, filled = data['progress']
    top = ROW0 + (srow - 1) * LH + LH * 0.5
    rect(col0 + s0 * CW, top, (s1 - s0) * CW, 1.33, C['progress_track'])
    rect(col0 + s0 * CW, top - 0.33, filled * CW, 2, C['progress_fill'])

css = open(os.path.join(HERE, 'mockup.css')).read()
a('<!doctype html><html><head><meta charset="utf-8"><title>midterm 1:1 mockup (logical px)</title>')
a(f'<style>{css}</style></head><body><div id="win">')

# window + sidebar
rect(0, 0, SB - 1, H, C['sidebar_bg'])
rect(SB - 1, 0, 1, H, C['sidebar_border'])
# macOS traffic lights (Windows: leave empty, see spec)
for cx, col in ((15.0, '#F45047'), (37.0, '#FFB400'), (59.0, '#1EB802')):
    a(f'<div class="r" style="left:{cx-6.5:.2f}px;top:{19-6.5:.2f}px;width:13px;height:13px;border-radius:50%;background:{col}"></div>')
icon('i_sidebar')
# New session button
rect(6, 45.33, 260, 30, C['button_bg'], 6, f'box-shadow:inset 0 1px 0 {C["button_highlight"]}')
icon('i_newsession')
ui_text(42.67, 66.0, 'New session', F['ui_size'], C['text'])
ui_text(254.7, 66.0, '⇧⌘T', F['ui_size'], C['text_muted'], anchor='right')
rect(273.33, 45.33, 30, 30, C['button_bg'], 6, f'box-shadow:inset 0 1px 0 {C["button_highlight"]}')
icon('i_folder')
# Search
rect(6, 86, 297.33, 29.33, C['input_bg'], 6, f'box-shadow:inset 0 0 0 1px {C["input_border"]}')
icon('i_search')
ui_text(42.67, 106.0, 'Search agents', F['ui_size'], C['text_muted'])
rect(279.33, 91.33, 20.67, 18.67, C['kbd_bg'], 4, f'box-shadow:inset 0 0 0 1px {C["kbd_border"]}')
ui_text(285.0, 106.0, '/', F['ui_size'], C['text_muted'])
# Segmented filter
rect(6, 123.33, 297.33, 28, C['input_bg'], 6, f'box-shadow:inset 0 0 0 1px {C["input_border"]}')
rect(8, 125.33, 96.67, 24, C['segment_active'], 5)
ui_text(42.67, 143.3, 'All', F['ui_size'], C['text'])
ui_text(62.67, 143.3, '6', F['ui_size'], C['text_muted'])
icon('i_waiting')
ui_text(139.33, 143.3, 'Waiting', F['ui_size'], C['text_muted'])
icon('i_working')
ui_text(229.33, 143.3, 'Working', F['ui_size'], C['text_muted'])
ui_text(284.67, 143.3, '5', F['ui_size'], C['text_muted'])
# Groups + rows
groups = [
  ('bid-model-detector', [('row_claude_done', 'Logos que no se muestran en línea', 'Done', 'ok', '3m')]),
  ('midterm', [('row_claude_working', 'Claude Code', 'Converting screenshot to PNG', 'text_muted', '21m'),
               ('row_openai_working', 'midterm', 'Working', 'text_muted', '1m')]),
  ('midulive', [('row_claude_working', 'Claude Code', 'Writing crates/platform/src/vision.rs', 'text_muted', '17m'),
                ('row_openai_working', 'midulive', 'Working', 'text_muted', '27s')]),
  ('midu-videos', [('row_claude_working', 'Claude Code', 'Reading style memory, clip sync fields and ig…', 'text_muted', '3m')]),
]
S = T['sidebar']
hdr_base = S['first_group_baseline']
for gname, rows in groups:
    ui_text(13.33, hdr_base, gname, F['group_size'], C['text_muted'])
    tb = hdr_base + S['group_to_title_baseline']
    for ic, title, sub, subc, tm in rows:
        icon(ic, 10.67, tb - S['icon_above_baseline'])
        ui_text(42.67, tb, title, F['ui_size'], C['text'], width=226, extra='overflow:hidden;text-overflow:ellipsis')
        ui_text(42.67, tb + S['title_to_sub'], sub, F['sub_size'], C[subc], width=250, extra='overflow:hidden;text-overflow:ellipsis')
        ui_text(296.0, tb, tm, F['sub_size'], C['text_muted'], anchor='right')
        tb += S['row_pitch']
    hdr_base = tb - S['row_pitch'] + S['last_title_to_next_group']
# Launchers row + Archived
rect(0, 941.33, SB - 1, 1, C['sidebar_divider'])
for n in ('l_claude', 'l_openai', 'l_grok', 'l_antigravity', 'l_copilot', 'l_cube', 'l_opencode', 'l_junie'):
    icon(n)
rect(0, 987.33, SB - 1, 1, C['sidebar_divider'])
icon('i_archive')
ui_text(36.0, 1016.0, 'Archived', F['ui_size'], C['text_muted'])
ui_text(293.3, 1016.0, '19', F['sub_size'], C['text_muted'], anchor='right')

# Tab bar
rect(SB, 0, W - SB, TAB_H, C['tabbar_bg'])
rect(SB, TAB_H - 1, W - SB, 1, C['tabbar_border'])
TW = 224.0
tabs = [('tab_claude_active', 'Logos que no se muestran en línea', True), ('tab_claude', 'Claude Code', False),
        ('tab_claude', 'Claude Code', False), ('tab_openai', 'midulive', False), ('tab_openai', 'midterm', False)]
for i, (ic, title, active) in enumerate(tabs):
    x = SB + i * TW
    if active:
        rect(x, 0, TW, TAB_H, C['term_bg'])
        rect(x + TW, 0, 1, TAB_H, C['tab_active_edge'])
        icon(ic, x + 5.33, 8.0)
        a(f'<div class="t fade" style="left:{x+26.67:.2f}px;top:{25.3-0.9636*F["ui_size"]:.2f}px;width:{TW-34:.2f}px;font-size:{F["ui_size"]}px;color:{C["text"]}">{esc(title)}</div>')
    else:
        if i > 1:
            rect(x, 12, 1, 14.67, C['tab_separator'])
        a(f'<div class="t tabc" style="left:{x:.2f}px;top:{25.3-0.9636*F["ui_size"]:.2f}px;width:{TW}px;font-size:{F["ui_size"]}px;color:{C["text_muted"]}"><img src="assets/{ic}.png" style="width:20px;height:20px;vertical-align:-5.3px;margin-right:0.5px">{esc(title)}</div>')
icon('i_plus'); icon('i_activity'); icon('i_globe')

# Panes
rect(SB, TAB_H, DIV_X - SB, H - TAB_H, C['term_bg'])
rect(DIV_X, TAB_H, 1, H - TAB_H, C['pane_divider'])
rect(DIV_X + 1, TAB_H, W - DIV_X - 1, H - TAB_H, C['term_bg'])
a('<div class="pane" id="left">'); pane(LCOL0, LEFT, True); a('</div>')
a('<div class="pane dim" id="right">'); pane(RCOL0, RIGHT, False); a('</div>')
a(f'<div class="r" style="left:0;top:0;width:{W}px;height:{H}px;border-radius:10px;box-shadow:inset 0 0 0 1px {C["window_border"]};pointer-events:none"></div>')
a('</div></body></html>')
open(os.path.join(ROOT, 'mockup.html'), 'w', encoding='utf-8', newline='\n').write('\n'.join(out))
print('ok')
