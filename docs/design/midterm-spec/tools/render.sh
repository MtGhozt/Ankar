#!/bin/sh
# Renders mockup.html at device scale 1.5 (same pixel density as the reference screenshot)
cd "$(dirname "$0")/.."
google-chrome --headless=new --no-sandbox --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=1.5 --window-size=1838,1034 \
  --screenshot="$PWD/mockup@1.5x.png" "file://$PWD/mockup.html" 2>/dev/null
