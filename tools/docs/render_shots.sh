#!/usr/bin/env bash
# Render the user-manual images.
#
#   tools/docs/render_shots.sh [--no-build] [--no-registry] [--no-gui]
#
# 1. builds the release CLI (and GUI unless --no-gui)
# 2. exports the feature registry   (ezyx docs-registry -o docs/manual/registry.json)
# 3. renders the headless fallback  (ezyx demo + ezyx sim -> docs/manual/src/img/cli_sim_preview.png)
# 4. renders every registry screenshot with the GUI under xvfb (EZYX_DOC_SHOTS)
# 5. copies the fallback image over any screenshot that did not render (update_manual.py placeholders;
#    skipped with --no-gui)
#
# Exit status: 0 = everything rendered, 2 = GUI screenshots failed (placeholders used), 1 = hard failure.
# Env: SCREEN (default 1480x920x24), SHOT_TIMEOUT seconds (default 900), CARGO (default cargo).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

build=1 registry=1 gui=1
for arg in "$@"; do
  case "$arg" in
    --no-build) build=0 ;;
    --no-registry) registry=0 ;;
    --no-gui) gui=0 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

CARGO="${CARGO:-cargo}"
REG="docs/manual/registry.json"
OUT="docs/manual/src/img"
CLI="target/release/ezyx"
APP="target/release/ez-yx-craft"
SCREEN="${SCREEN:-1480x920x24}"
SHOT_TIMEOUT="${SHOT_TIMEOUT:-900}"
mkdir -p "$OUT"

if [[ $build == 1 ]]; then
  pkgs=(-p ezyx-cli)
  [[ $gui == 1 ]] && pkgs+=(-p ezyx-app)
  "$CARGO" build --release "${pkgs[@]}"
fi

if [[ $registry == 1 ]]; then
  "$CLI" docs-registry -o "$REG"
fi

# Headless render: needs no GPU and always works, so the manual is never without a picture.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
"$CLI" demo -o "$tmp/demo.ezyx"
"$CLI" sim "$tmp/demo.ezyx" -o "$OUT/cli_sim_preview.png"
echo "rendered $OUT/cli_sim_preview.png"

status=0
if [[ $gui == 1 ]]; then
  run_gui=(env WGPU_BACKEND=gl EZYX_DOC_SHOTS="$REG" EZYX_DOC_SHOTS_OUT="$OUT" "$APP")
  if command -v xvfb-run >/dev/null 2>&1; then
    runner=(xvfb-run -a -s "-screen 0 $SCREEN")
  elif [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
    runner=()
  else
    echo "no display and no xvfb-run: skipping GUI screenshots" >&2
    runner=(false)
  fi
  if command -v timeout >/dev/null 2>&1 && [[ "${runner[0]:-}" != false ]]; then
    runner=(timeout "$SHOT_TIMEOUT" "${runner[@]}")
  fi
  if "${runner[@]}" "${run_gui[@]}"; then
    echo "GUI screenshots rendered into $OUT"
  else
    echo "GUI screenshot rendering failed; using the CLI preview as a placeholder" >&2
    status=2
  fi
fi

if [[ $gui == 1 ]]; then
  python3 tools/docs/update_manual.py placeholders --fallback "$OUT/cli_sim_preview.png"
fi
exit $status
