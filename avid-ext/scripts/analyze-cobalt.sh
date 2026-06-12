#!/bin/bash
# Analyse autonome de cobalt.tools — utilise AVID ScoutEngine via API

set -e
REPORT_DIR="/mnt/nvme/soullink_brain/analysis/cobalt"
mkdir -p "$REPORT_DIR"

TARGET="https://cobalt.tools/"
TIMESTAMP=$(date -Iseconds)

echo "════════════════════════════════════════════════════════"
echo "  ANALYSE: $TARGET"
echo "  Timestamp: $TIMESTAMP"
echo "════════════════════════════════════════════════════════"

# 1) Fetch the main page
echo "[1/5] Récupération de la page principale..."
curl -sL -o "$REPORT_DIR/index.html" \
  -H "User-Agent: AVID-Scout/1.0 (research)" \
  "$TARGET" 2>&1
echo "  Taille: $(wc -c < $REPORT_DIR/index.html) octets"

# 2) Extract links
echo "[2/5] Extraction des liens..."
grep -oP 'href="([^"]+)"' "$REPORT_DIR/index.html" | \
  grep -v '^#' | sort -u > "$REPORT_DIR/links.txt"
grep -oP 'src="([^"]+)"' "$REPORT_DIR/index.html" | sort -u > "$REPORT_DIR/assets.txt"
echo "  Liens: $(wc -l < $REPORT_DIR/links.txt)"
echo "  Assets: $(wc -l < $REPORT_DIR/assets.txt)"

# 3) Extract structured data
echo "[3/5] Extraction des métadonnées..."
python3 << 'PYEOF' 2>/dev/null | tee "$REPORT_DIR/metadata.txt"
import re, json
with open('/mnt/nvme/soullink_brain/analysis/cobalt/index.html') as f:
    content = f.read()

# Title
m = re.search(r'<title>([^<]+)', content)
title = m.group(1) if m else "N/A"

# Description
m = re.search(r'<meta[^>]*name=["\']description["\'][^>]*content=["\']([^"\']+)', content)
desc = m.group(1) if m else "N/A"

# Open Graph
og = {}
for m in re.finditer(r'<meta[^>]*property=["\'](og:[^"\']+)["\'][^>]*content=["\']([^"\']+)', content):
    og[m.group(1)] = m.group(2)

# Script tags (JS bundles)
scripts = re.findall(r'<script[^>]*src=["\']([^"\']+)', content)

# API endpoints detection
api_patterns = re.findall(r'(?:api|graphql|rest|v1|v2|/api/|/graphql)', content, re.I)

print(f"Titre: {title}")
print(f"Description: {desc[:200] if desc != 'N/A' else desc}")
if og: print(f"OpenGraph: {json.dumps(og, indent=2)}")
print(f"Scripts: {len(scripts)} bundles")
print(f"APIs: {api_patterns}" if api_patterns else "No API endpoints detected")
PYEOF

# 4) Check for JavaScript frameworks and tech stack
echo "[4/5] Détection de la stack technique..."
grep -Eo '(react|next\.?js?|vue|svelte|angular|nuxt|gatsby|tailwind|sass|scss|webpack|vite|esbuild)' \
  "$REPORT_DIR/index.html" | sort -u > "$REPORT_DIR/techstack.txt"
echo "  Stack: $(cat $REPORT_DIR/techstack.txt | tr '\n' ' ')"

# 5) Check API availability
echo "[5/5] Vérification des endpoints API..."
for endpoint in api graphql v1 v2 rest api/v1; do
  code=$(curl -s -o /dev/null -w "%{http_code}" "https://cobalt.tools/$endpoint" 2>/dev/null || echo "000")
  echo "  /$endpoint → HTTP $code"
done

echo ""
echo "════════════════════════════════════════════════════════"
echo "  RAPPORT COMPLET: $REPORT_DIR/metadata.txt"
echo "════════════════════════════════════════════════════════"