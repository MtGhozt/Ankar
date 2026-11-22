#!/usr/bin/env python3
"""Annotated crops of the reference. Boxes/labels are in LOGICAL px."""
import os
from PIL import Image, ImageDraw, ImageFont
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ref = Image.open(os.path.join(R, 'ref-window@1.5x.png')).convert('RGB')
S = 1.5; Z = 2  # crop is upscaled 2x for readability -> 3 image px per logical px
try:
    font = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', 15)
except Exception:
    font = ImageFont.load_default()
COLS = [(255, 0, 200), (0, 220, 255), (255, 200, 0), (120, 255, 80)]
def crop(name, area, boxes, pad_right=330):
    x0, y0, x1, y1 = area
    c = ref.crop((int(x0*S), int(y0*S), int(x1*S), int(y1*S)))
    c = c.resize((c.width*Z, c.height*Z), Image.LANCZOS)
    canvas = Image.new('RGB', (c.width + pad_right, max(c.height, 30 + 20*len(boxes))), (24, 24, 24))
    canvas.paste(c, (0, 0)); d = ImageDraw.Draw(canvas)
    k = S*Z
    for i, (bx, label) in enumerate(boxes):
        col = COLS[i % len(COLS)]
        X0, Y0, X1, Y1 = [(v - o)*k for v, o in zip(bx, (x0, y0, x0, y0))]
        d.rectangle((X0, Y0, X1, Y1), outline=col, width=1)
        d.text((X0 + 2, Y0 + 1), str(i+1), fill=col, font=font)
        d.text((c.width + 10, 8 + i*20), f'{i+1}. {label}', fill=col, font=font)
    canvas.save(os.path.join(R, 'crops', name + '.png'))
crop('01-sidebar-full', (0, 0, 312, 1033.33), [
    ((0, 0, 309.33, 1033.33), 'sidebar 0..309.33 (1px border at 308.33 #1C1C1C), bg #0C0C0C'),
    ((0, 0, 309.33, 38), 'title strip 38px (mac traffic lights; Windows: keep empty)'),
    ((6, 45.33, 266, 75.33), 'New session button 260x30 r6 #181818'),
    ((273.33, 45.33, 303.33, 75.33), 'folder+ button 30x30 r6'),
    ((6, 86, 303.33, 115.33), 'Search 297x29.3 r6 #080808 + 1px #141414'),
    ((6, 123.33, 303.33, 151.33), 'Filter segmented 297x28 r6'),
    ((0, 165, 309.33, 560), 'groups + session rows (row pitch 45.33)'),
    ((0, 941.33, 309.33, 987.33), 'launcher row 46px between 1px #191919 lines'),
    ((0, 987.33, 309.33, 1033.33), 'Archived row 46px'),
], pad_right=560)
crop('02-new-session-search', (0, 38, 312, 156), [
    ((6, 45.33, 266, 75.33), 'button 260x30 r6 #181818, inset top 1px #1E1E1E'),
    ((18, 52, 34, 68.67), 'square-pen icon 16px #E7E7E7 at x18'),
    ((42.67, 56.7, 120, 66), 'New session 13px #E7E7E7 baseline y66'),
    ((224, 56.7, 254.7, 66), 'shortcut 13px #838383, right edge 254.7'),
    ((273.33, 45.33, 303.33, 75.33), 'folder-plus 30x30, icon 17px #7E7E7E'),
    ((6, 86, 303.33, 115.33), 'search 29.3 tall r6 bg #080808 border #141414'),
    ((42.67, 96, 127.3, 108.7), 'Search agents 13px #838383 baseline 106'),
    ((279.33, 91.33, 300, 110), 'kbd hint 20.7x18.7 r4 #141414 border #1E1E1E'),
], pad_right=520)
crop('03-filter-chips', (0, 118, 312, 156), [
    ((6, 123.33, 303.33, 151.33), 'container 297.3x28 r6 #080808 + 1px #141414'),
    ((8, 125.33, 104.67, 149.33), 'active segment 96.7x24 r5 #202020'),
    ((42.67, 134, 57, 143.3), 'All 13px #E7E7E7 + count 6 #838383 (x 62.7)'),
    ((123.33, 132.67, 134, 143.33), 'waiting dot ~7px #454545'),
    ((139.33, 132.67, 184.7, 145.3), 'Waiting 13px #838383'),
    ((212, 130.67, 225.33, 144.67), 'spinner 13px (dashed ring)'),
    ((229.33, 132.67, 291.3, 145.3), 'Working 5'),
], pad_right=460)
crop('04-session-rows', (0, 165, 312, 360), [
    ((13.33, 174, 130, 183.33), 'group header 12px #838383 x13.3 baseline 183.3'),
    ((10.67, 192, 37.33, 218.67), 'agent icon 18px + status badge (box 26.7)'),
    ((42.67, 201.3, 250, 211.33), 'title 13px #E7E7E7 baseline +28 from header'),
    ((42.67, 219.3, 72, 228), 'subtitle 12px baseline +16.67 (Done = #6FA77D)'),
    ((274, 202, 296, 211.33), 'time 12px #838383 right edge 296'),
    ((0, 270, 309, 315.33), 'row pitch 45.33 (no hover/selected bg visible)'),
], pad_right=500)
crop('05-launcher-row-archived', (0, 930, 312, 1033.33), [
    ((0, 941.33, 309.33, 942.33), '1px #191919'),
    ((16.67, 951.33, 38.67, 973.33), 'launcher icon ~18px + green ring badge, pitch 35px'),
    ((0, 987.33, 309.33, 988.33), '1px #191919'),
    ((11.33, 1001.33, 31.33, 1020), 'archive icon 18px #838383'),
    ((36, 1005.3, 88.7, 1016), 'Archived 13px #838383 baseline 1016'),
    ((280.7, 1006.7, 293.3, 1016), 'count 12px right edge 293.3'),
], pad_right=520)
crop('06-tab-bar', (306, 0, 1000, 40), [
    ((310.67, 0, 534.67, 38), 'active tab 224 wide, bg #000 (= terminal), no bottom line'),
    ((316, 8, 336, 28), 'tab icon 16px + badge (box 20)'),
    ((337.3, 14.7, 498, 25.3), 'title 13px #E7E7E7, left aligned, fades out'),
    ((534.67, 0, 758.67, 38), 'inactive tab 224, bg #0C0C0C, content centered #838383'),
    ((757.33, 12, 758.33, 26.67), 'separator 1x14.7 #151515 (between inactive)'),
    ((310.67, 37, 1000, 38), 'bottom border 1px #1C1C1C (not under active)'),
], pad_right=560)
crop('07-tab-bar-right', (1420, 0, 1837.33, 40), [
    ((1443.33, 14, 1455.33, 26), '+ 12px #838383 (after last tab)'),
    ((1770, 10.67, 1789.33, 28.67), 'activity icon 18px'),
    ((1804.67, 10, 1824, 29.33), 'globe icon 18px'),
], pad_right=330)
crop('08-pane-status-bar', (310, 925, 1000, 1033.33), [
    ((324.67, 937.6, 980, 955.3), 'row 51: ─ separator (TUI glyph)'),
    ((324.67, 955.3, 360, 973), 'row 52: prompt ❯ + bar cursor 2px #0F8CEC'),
    ((324.67, 990.7, 980, 1008.5), 'row 54: status line (drawn by Claude Code)'),
    ((618, 998, 704, 1001), 'progress: 15% #C87358 on #494949'),
], pad_right=470)
crop('09-diff-bands', (310, 38, 1000, 260), [
    ((322.4, 53.33, 961.3, 71.07), 'row 1: user msg bg #303030, fg #F8F8F8'),
    ((367.8, 71.07, 920, 195.2), 'deleted: bg #350400, line no. #C5555B'),
    ((367.8, 195.2, 920, 260), 'added: bg #062401, line no. #50B950'),
], pad_right=440)
crop('10-split-divider-dim', (900, 38, 1100, 300), [
    ((998, 38, 999, 300), 'divider 1px #1C1C1C'),
    ((999, 38, 1100, 300), 'unfocused pane: everything x0.6 brightness'),
], pad_right=420)
print('crops ok')
