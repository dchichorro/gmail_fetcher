import sys, json
from PIL import Image, ImageDraw, ImageFont

summary = json.load(sys.stdin)
output_path = sys.argv[1]
filter_name = sys.argv[2] if len(sys.argv) > 2 else "all"

title = f"{filter_name.title()} Spending Over Time" if filter_name != "all" else "Spending Over Time"

months = [m for m, _ in summary]
totals = [t for _, t in summary]
max_val = max(totals) if totals else 1

W, H = 1600, 700
MARGIN_L, MARGIN_R, MARGIN_T, MARGIN_B = 90, 30, 70, 100
chart_w = W - MARGIN_L - MARGIN_R
chart_h = H - MARGIN_T - MARGIN_B

img = Image.new("RGB", (W, H), (255, 255, 255))
draw = ImageDraw.Draw(img)

try:
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 13)
    font_title = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 24)
    font_small = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 11)
except:
    font = ImageFont.load_default()
    font_title = font
    font_small = font

# Title
bbox = draw.textbbox((0, 0), title, font=font_title)
tw = bbox[2] - bbox[0]
draw.text(((W - tw) // 2, 15), title, fill=(0, 0, 0), font=font_title)

# Y-axis gridlines and labels
for i in range(6):
    val = max_val * 1.1 * i / 5
    y = MARGIN_T + chart_h - (i / 5) * chart_h
    draw.line([(MARGIN_L, y), (W - MARGIN_R, y)], fill=(220, 220, 220), width=1)
    draw.text((10, y - 8), f"\u20ac{val:.0f}", fill=(80, 80, 80), font=font_small)

bar_w = max(1, chart_w // len(totals) - 1) if totals else 1

# Bars
for i, (m, t) in enumerate(summary):
    x = MARGIN_L + i * (chart_w / len(summary))
    bar_h = (t / (max_val * 1.1)) * chart_h
    y_top = MARGIN_T + chart_h - bar_h
    y_bot = MARGIN_T + chart_h
    intensity = min(t / max_val, 1.0)
    r = int(40 + 180 * intensity)
    g = int(120 * (1 - 0.5 * intensity))
    b = int(200 - 150 * intensity)
    draw.rectangle([x, y_top, x + bar_w, y_bot], fill=(r, g, b))

# 3-month moving average line
avg3 = []
for i in range(len(totals)):
    s = max(0, i - 1)
    e = min(len(totals), i + 2)
    avg3.append(sum(totals[s:e]) / (e - s))

points = []
for i, v in enumerate(avg3):
    x = MARGIN_L + i * (chart_w / len(summary)) + bar_w / 2
    y = MARGIN_T + chart_h - (v / (max_val * 1.1)) * chart_h
    points.append((x, y))

for i in range(len(points) - 1):
    draw.line([points[i], points[i + 1]], fill=(220, 50, 50), width=2)

# X-axis labels (every 3rd month)
for i, (m, _) in enumerate(summary):
    if i % 3 == 0:
        x = MARGIN_L + i * (chart_w / len(summary))
        draw.text((x, MARGIN_T + chart_h + 10), m, fill=(80, 80, 80), font=font_small, anchor="mt")

# Legend
draw.rectangle([MARGIN_L + 20, MARGIN_T + 10, MARGIN_L + 40, MARGIN_T + 22], fill=(60, 100, 180))
draw.text((MARGIN_L + 45, MARGIN_T + 8), "Monthly", fill=(0, 0, 0), font=font_small)
draw.line([(MARGIN_L + 130, MARGIN_T + 16), (MARGIN_L + 160, MARGIN_T + 16)], fill=(220, 50, 50), width=2)
draw.text((MARGIN_L + 165, MARGIN_T + 8), "3-month avg", fill=(0, 0, 0), font=font_small)

img.save(output_path, "JPEG", quality=95)
print(f"Saved {output_path} ({img.size[0]}x{img.size[1]})")
