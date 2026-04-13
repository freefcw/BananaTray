#!/usr/bin/env bash
#
# 创建 DMG 背景图片（使用系统工具）
#

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_FILE="$SCRIPT_DIR/dmg-background.png"

# 使用 sips 创建简单的渐变背景
if command -v sips >/dev/null 2>&1; then
    # 创建基础图片
    /usr/bin/python3 -c "
from Cocoa import NSImage, NSBitmapImageRep, NSPNGFileType, NSCalibratedRGBColorSpace
import AppKit

# 创建 800x600 图片
image = AppKit.NSImage.alloc().initWithSize_((800, 600))
image.lockFocus()

# 设置背景色
AppKit.NSColor.colorWithCalibratedRed_green_blue_alpha_(0.96, 0.96, 0.96, 1.0).set()
AppKit.NSBezierPath.fillRect_(((0, 0), (800, 600)))

# 添加标题
font = AppKit.NSFont.fontWithName_size_('Helvetica', 48)
AppKit.NSColor.colorWithCalibratedRed_green_blue_alpha_(0.2, 0.2, 0.2, 1.0).set()
title = 'BananaTray'
title_size = title.sizeWithAttributes_({AppKit.NSFontAttributeName: font})
title_x = (800 - title_size.width) / 2
title.drawAtPoint_withAttributes_((title_x, 400), {AppKit.NSFontAttributeName: font})

# 添加副标题
font_small = AppKit.NSFont.fontWithName_size_('Helvetica', 24)
AppKit.NSColor.colorWithCalibratedRed_green_blue_alpha_(0.4, 0.4, 0.4, 1.0).set()
subtitle = 'AI Coding Assistant Quota Monitor'
subtitle_size = subtitle.sizeWithAttributes_({AppKit.NSFontAttributeName: font_small})
subtitle_x = (800 - subtitle_size.width) / 2
subtitle.drawAtPoint_withAttributes_((subtitle_x, 350), {AppKit.NSFontAttributeName: font_small})

# 添加说明
font_tiny = AppKit.NSFont.fontWithName_size_('Helvetica', 16)
AppKit.NSColor.colorWithCalibratedRed_green_blue_alpha_(0.5, 0.5, 0.5, 1.0).set()
instruction = 'Drag the app to Applications folder to install'
inst_size = instruction.sizeWithAttributes_({AppKit.NSFontAttributeName: font_tiny})
inst_x = (800 - inst_size.width) / 2
instruction.drawAtPoint_withAttributes_((inst_x, 100), {AppKit.NSFontAttributeName: font_tiny})

image.unlockFocus()

# 保存为 PNG
bitmap = AppKit.NSBitmapImageRep.alloc().initWithData_(image.TIFFRepresentation())
data = bitmap.representationUsingType_properties_(AppKit.NSPNGFileType, None)
data.writeToFile_atomically_('$OUTPUT_FILE', True)
" 2>/dev/null || echo "Python Cocoa 方法失败，使用备用方案"
fi

# 备用方案：使用 ImageMagick（如果可用）
if [ ! -f "$OUTPUT_FILE" ] && command -v convert >/dev/null 2>&1; then
    convert -size 800x600 xc:'#f5f5f5' \
        -font Helvetica -pointsize 48 -fill '#333333' -gravity north \
        -annotate +0+200 'BananaTray' \
        -font Helvetica -pointsize 24 -fill '#666666' \
        -annotate +0+260 'AI Coding Assistant Quota Monitor' \
        -font Helvetica -pointsize 16 -fill '#888888' -gravity south \
        -annotate +0+100 'Drag the app to Applications folder to install' \
        "$OUTPUT_FILE"
fi

# 最后备用方案：创建一个简单的纯色背景
if [ ! -f "$OUTPUT_FILE" ]; then
    # 使用 sips 创建纯色背景
    if command -v sips >/dev/null 2>&1; then
        # 创建一个 1x1 的基础图片
        /usr/bin/python3 -c "
import sys
try:
    from PIL import Image
    img = Image.new('RGB', (800, 600), '#f5f5f5')
    img.save('$OUTPUT_FILE', 'PNG')
except ImportError:
    # 如果没有 PIL，创建一个最小的 PNG
    import base64
    # 1x1 灰色 PNG 的 base64
    png_data = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==')
    with open('$OUTPUT_FILE', 'wb') as f:
        f.write(png_data)
" 2>/dev/null || touch "$OUTPUT_FILE"
    else
        touch "$OUTPUT_FILE"
    fi
fi

if [ -f "$OUTPUT_FILE" ]; then
    echo "✅ DMG 背景图片已创建: $OUTPUT_FILE"
    echo "   大小: $(du -h "$OUTPUT_FILE" | cut -f1)"
else
    echo "⚠️  无法创建 DMG 背景图片，将使用默认背景"
fi
