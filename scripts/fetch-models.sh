#!/usr/bin/env bash
# ONNX modellarini OpenCV Model Zoo'dan yuklab, SHA-256 bo'yicha tekshiradi.
# Modellar git'da saqlanmaydi (hajmi katta); yangi klonda bir marta ishga tushiring.
#   YuNet — MIT, SFace — Apache-2.0
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)/frontend/src-tauri/resources/models"
BASE="https://github.com/opencv/opencv_zoo/raw/main/models"
mkdir -p "$DIR"

fetch() { # <zoo yo'li> <sha256>
  local name; name="$(basename "$1")"
  local dest="$DIR/$name"
  if [[ -f "$dest" ]] && echo "$2  $dest" | sha256sum -c --status; then
    echo "bor: $name"; return
  fi
  echo "yuklanmoqda: $name"
  curl -fsSL -o "$dest.part" "$BASE/$1"
  echo "$2  $dest.part" | sha256sum -c --status || { echo "SHA-256 mos emas: $name" >&2; rm -f "$dest.part"; exit 1; }
  mv "$dest.part" "$dest"
}

fetch face_detection_yunet/face_detection_yunet_2023mar.onnx 8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4
fetch face_recognition_sface/face_recognition_sface_2021dec.onnx 0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79
