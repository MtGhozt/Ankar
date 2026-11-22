# midterm 1:1 — visual spec for Ankar

Target: Ankar must look **identical** to midudev's *midterm* terminal screenshot. Personal styling comes later.

## Files

| File | What it is |
|---|---|
| `ref-orig.jpg` | Original screenshot (3000×1794, `pbs.twimg.com/media/HXlXtF5XoAAs98Z.jpg?name=orig`). Post: https://x.com/midudev/status/2123825139405979823 |
| `ref-window@1.5x.png` | Window cropped out of the screenshot: 2756×1550 physical px (= 1837.33×1033.33 logical) |
| `ref-window@1x.png` | Same, downscaled to logical size |
| `mockup.html` | Static HTML/CSS recreation. Every element is absolutely positioned in **logical px** |
| `mockup.png` / `mockup@1.5x.png` | Mockup rendered with headless Chrome at 1× and 1.5× |
| `compare.png` | Reference (left) vs mockup@1.5x (right), full window |
| `crops/NN-*.png` | Annotated crops of the reference (2× zoom). Numbers in the boxes match the legend |
| `crops/cmp-*.png` | Reference vs mockup side by side, per area |
| `assets/*.png` | Icons cut out of the reference (alpha un-matted, 1.5× density). Use them **only as visual reference**; Ankar uses its own icon assets |
| `tools/` | `gen.py` (builds the HTML from `tokens.json` + `term_*.json`), `render.sh`, `compare.py`, `crops.py` |

## Coordinate system (read this first)

- The screenshot is **1.5 physical px per logical px**. This is measured, not assumed:
  - the macOS traffic lights are 19.5 px (≈ 13 pt);
  - "New session" has a cap height of 15 px and a width of 111 px, which matches a 13 px UI font;
  - the mono cell is 12.95×26.6 px, i.e. 8.63×17.73 logical.
- **All numbers in this spec are logical px** (egui "points").
- Windows at **150 % scaling** has `pixels_per_point = 1.5`. That is the same density as the reference, so an Ankar screenshot at 150 % with the window at 1837×1033 logical (2756×1550 px client area) can be overlaid 1:1 on `ref-window@1.5x.png`.
- Colors were sampled with PIL from the JPEG. They are real pixel values, with ±2 per channel of JPEG noise.
- Fonts: the reference uses SF Pro (UI) and SF Mono/Menlo (terminal), which can't be shipped. The stand-ins are **Inter** (UI) and **Hack** (mono). Hack is a Menlo/DejaVu-derived font with the same 0.602 em advance.

## Layout (logical px, window 1837.33 × 1033.33)

```
x=0                     309.33                                     998 999                          1837.33
y=0  ┌────────────────────┬──────────┬────────┬────────┬────────┬────────┬──┬──────────────────┬──────┐
     │ ● ● ●  [⊟]         │ ACTIVE   │ tab 2  │ tab 3  │ tab 4  │ tab 5  │+ │                  │ ∿  ⊕ │  tab bar 38 (border-bottom 1px @37)
y=38 ├────────────────────┤ 224 wide each, starting x=310.67 ── + at 1449 ─┴──────────────────┴──────┤
     │ [✎ New session ⇧⌘T][📁]  y 45.33–75.33 │                                │                    │
     │ [🔍 Search agents   / ]  y 86–115.33   │  LEFT PANE (focused)           │ RIGHT PANE         │
     │ [ All 6 | ● Waiting | ◌ Working 5 ] y 123.33–151.33                     │ (unfocused,        │
     │ bid-model-detector       baseline 183.33│  col0 x=324.67, row1 y=53.33   │  brightness ×0.6)  │
     │ ✳ Logos que no…   3m     baseline 211.33│  cell 8.627 × 17.733           │  col0 x=1013.33    │
     │   Done                   baseline 228   │  78 cols × 55 rows             │  95 cols × 55 rows │
     │ …row pitch 45.33, group gap +52.67…     │                                │                    │
     │                                         │                                │                    │
y=941├────────────────────┤ 1px #191919        │                                │                    │
     │ ✳ ◎ ⊘ A 👾 ⬡ ▯ ✚   launchers y 951–974  │  row 51 ───── row 52 ❯ ▏       │                    │
y=987├────────────────────┤ 1px #191919        │  row 53 ───── row 54 status    │                    │
     │ 🗄 Archived      19  baseline 1016      │                                │                    │
y=1033└────────────────────┴──────────────────────────────────────────────┴─┴────────────────────────┘
```

- Sidebar: x 0 → 308.33, with a 1 px border `#1C1C1C` at x 308.33.
- Terminal area: x 309.33 → 998. Pane divider: 1 px `#1C1C1C` at x 998. Right pane: x 999 → 1837.33.
- The window has a 1 px border `#1D1D1D` and radius 10 (macOS). On Windows 11 the OS rounds the corners.

## Global tokens

| Token | Value | Used for |
|---|---|---|
| `window_bg` / `term_bg` | `#000000` | Window, both panes, active tab |
| `sidebar_bg` / `tabbar_bg` | `#0C0C0C` | Sidebar, inactive tabs, tab bar |
| `sidebar_border` / `tabbar_border` / `pane_divider` | `#1C1C1C` | 1 px lines |
| `sidebar_divider` | `#191919` | Lines above and below the launcher row |
| `button_bg` | `#181818` | New session, folder+ |
| `button_highlight` | `#1E1E1E` | 1 px inner top line of buttons |
| `input_bg` | `#080808` | Search box, filter container |
| `input_border` | `#141414` | 1 px inner border of search/filter |
| `kbd_bg` / `kbd_border` | `#141414` / `#1E1E1E` | `/` key hint |
| `segment_active` | `#202020` | Selected filter segment |
| `text` | `#E7E7E7` | Titles, button labels, active tab |
| `text_muted` | `#838383` | Placeholder, group headers, subtitles, times, counts, inactive tabs, icons |
| `ok` | `#6FA77D` | "Done" subtitle |
| `status_dot_ok` | `#53BE6B` | Green badge on the agent icon |
| `cursor` | `#0F8CEC` | Terminal cursor (2 px bar when focused, 1 px outline when not) |
| UI font | Inter, **13 px** (titles, buttons, tabs, placeholder, Archived), **12 px** (subtitles, group headers, times, counts) | weight ≈ 460: Inter Regular looks thinner than SF on macOS, so in egui use **Inter Medium** if Regular looks thin next to the crops |
| Mono font | Hack **14.33 px**, cell **8.627 × 17.733** | Terminal |

## Components

### Title strip (sidebar, y 0–38)
- macOS shows traffic lights at x 15/37/59 (Ø13). **On Windows, leave x 0–70 empty** (it's a window drag area if the custom title bar is on).
- Sidebar toggle icon (`panel-left`): 18.7 px box at (81.33, 10), color `text_muted`.

### New session button + folder button
| Property | Value |
|---|---|
| New session rect | x 6, y 45.33, w 260, h 30, radius 6, bg `#181818`, inner top line 1 px `#1E1E1E` |
| Icon | square-pen, 16 px, at (18, 52), color `text` |
| Label | "New session", 13 px `text`, x 42.67, baseline 66 |
| Shortcut | 13 px `text_muted`, right edge 254.7, baseline 66. Windows text: `Ctrl+Shift+T` (or `⇧^T`, shorter) |
| Folder+ rect | x 273.33, y 45.33, 30×30, radius 6, same bg. Icon folder-plus 17 px `text_muted` centered |

### Search
| Property | Value |
|---|---|
| Rect | x 6, y 86, w 297.33, h 29.33, radius 6, bg `#080808`, 1 px inner border `#141414` |
| Icon | search 12–13 px, at (18, 93.3), `text_muted` |
| Placeholder | "Search agents", 13 px `text_muted`, x 42.67, baseline 106 (**vertically centered**: text box center = rect center 100.67) |
| Key hint | rect x 279.33, y 91.33, 20.67×18.67, radius 4, bg `#141414`, 1 px border `#1E1E1E`; "/" 13 px `text_muted` centered |

### Filter (segmented control)
| Property | Value |
|---|---|
| Container | x 6, y 123.33, w 297.33, h 28, radius 6, bg `#080808`, 1 px inner border `#141414`, padding 2 |
| Segments | 3 equal (96.67 wide), radius 5. Selected bg `#202020` |
| "All 6" | "All" 13 px `text` at x 42.67, "6" 13 px `text_muted` at x 62.67, baseline 143.3 |
| "Waiting" | dot Ø7 `#454545` centered at (128.7, 138), label x 139.33 `text_muted` (count hidden when 0) |
| "Working 5" | dashed spinner ring Ø13 at (212, 130.67), label x 229.33, count x 284.67 |
| Glyphs | **Do not use font glyphs for the dot/spinner** (that's what produced "OEsp."/"GTrab."). Paint them with the painter |

### Group header + session row
| Property | Value |
|---|---|
| Group header | 12 px `text_muted`, x 13.33. First baseline 183.33 |
| Header → first title baseline | +28 |
| Title | 13 px `text`, x 42.67, ellipsis at x ≈ 270 (before the time) |
| Subtitle | 12 px, baseline = title + 16.67. Color `text_muted`, or `ok` when it says "Done" |
| Time | 12 px `text_muted`, right-aligned at x 296, same baseline as the title |
| Row pitch | 45.33 (title baseline → next title baseline) |
| Last title in a group → next header baseline | +52.67 |
| Agent icon | ≈ 18 px glyph centered at (24, title_baseline − 6.7), inside a 26.7 box at (10.67, title_baseline − 20) |
| Status badge | top-right of the icon. **Done**: filled circle Ø8 `#53BE6B`. **Working**: dashed ring Ø9, 8 dashes, `#909090`, rotating. Both have a 1.5 px ring of `sidebar_bg` around them |
| Hover / selected | Not visible in the reference. Use hover `#141414`, selected `#181818`, radius 6, inset x 6 |
| Text | Labels in **English**: "Done", "Working", "Waiting" |

### Launcher row + Archived
| Property | Value |
|---|---|
| Lines | 1 px `#191919` at y 941.33 and y 987.33 (full sidebar width) |
| Icons | glyph ≈ 16–18 px; first box (16.67, 951.33) 22×22; centers at x 27.7, 63, 99, 134, 169, 202, 238, 273 (pitch ≈ 35); center y ≈ 962.5 |
| Badge | green ring Ø9, stroke 2 `#65B177`, at the icon's top-right (launcher available) |
| Archived | archive icon 18 px at (11.33, 1001.33), "Archived" 13 px `text_muted` x 36 baseline 1016, count 12 px right-aligned at x 293.3 |

### Tab bar
| Property | Value |
|---|---|
| Bar | x 309.33 → 1837.33, h 38, bg `#0C0C0C`, bottom line 1 px `#1C1C1C` at y 37 (**not under the active tab**) |
| Tabs | **Fixed width 224**, first at x 310.67 (5 tabs end at 1430.67). No radius |
| Active | bg `#000000` (merges with the terminal), right edge 1 px `#1D1D1D`. Content **left-aligned**: icon (16 + badge, 20 box) at x+5.3, y 8; title 13 px `text` at x+26.7, baseline 25.33, fading out over the last 20 % (alpha gradient, no "…") |
| Inactive | bg `#0C0C0C`, content **centered** (icon + 1 px gap + title), title 13 px `text_muted`, baseline 25.33 |
| Separators | 1×14.67 px `#151515` at y 12, only between two inactive tabs |
| "+" | 12 px `text_muted` at (1443.33, 14), right after the last tab |
| Right icons | activity (1770, 10.67) and globe (1804.67, 10) at 18 px, `text_muted`. On Windows, if the custom title bar is on, the window buttons go **after** these |
| Removed | **No other row under the tab bar.** The "New session \| + New session" row must go |

### Terminal panes
| Property | Value |
|---|---|
| Padding | left 14 (col 0 at pane_x + 14), top 15.33 (row 1 top at y 53.33) |
| Cell | 8.627 × 17.733. Snap each row top to a physical pixel: `top_n = round((53.33 + n·17.733)·ppp)/ppp`. Each row's bottom must equal the next row's top, so background bands have **no seams** |
| Default fg / bg | `#E8E8E8` / `#000000` |
| Unfocused pane | everything ×0.6 brightness (measured: `#E8E8E8` → `#8C8C8C`, `#350400` → `#220501`). Paint a `#000000` rect at alpha 0.4 over the pane |
| Focus border | **None** |
| Cursor | focused: 2 px bar `#0F8CEC`, height cell − 3; unfocused: 1 px outlined box |
| Divider | 1 px `#1C1C1C` |

What's **inside** the panes (diff bands, ❯ input box, status line "▸▸ auto mode on · week ━━ 15% …") is **Claude Code's own TUI output**. Ankar doesn't draw it; it just has to render it exactly:
- 24-bit backgrounds, with no seams;
- box-drawing `─` drawn as a 1 px line through the middle of the cell;
- bold;
- 256/truecolor foregrounds.

Colors measured from the TUI output (useful to verify the renderer):

| Sample | Color |
|---|---|
| Deleted line bg / number | `#350400` / `#C5555B` |
| Added line bg / number | `#062401` / `#50B950` |
| User message bar bg / fg | `#303030` / `#F8F8F8` |
| Keyword (for/do/if/use/let) | `#D3305C` |
| Builtin/func (return/continue/Some) | `#A3D143` |
| Path/link/number | `#A8A9E1` |
| Comment | `#68665E` |
| String | `#CCCB7C` |
| Bullet (success) | `#3EB954` |
| "auto mode on" | `#E9B632` |
| Progress fill / track | `#C87358` / `#494949` |
| Muted text ("Ran 4 shell commands") | `#929292` |
| Separator `─` | `#7A7A7A` |

## How to re-render (optional)

```
python3 tools/terms.py && python3 tools/gen.py && tools/render.sh && python3 tools/compare.py
```
This needs Chrome. `compare.py x0 y0 x1 y1 out.png` compares a region in physical px of `ref-window@1.5x.png`.

## Known differences in the mockup (intentional or unavoidable)
- Inter is slightly wider and lighter than SF Pro, so the text runs ±1–3 px longer.
- The brand icons in the mockup are cut out of the JPEG, so they're slightly soft.
- The mac traffic lights appear in the mockup because they're in the reference. On Windows that area stays empty.
