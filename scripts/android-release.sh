#!/usr/bin/env bash
# Android release: ABI bo'yicha bo'lingan imzolangan APK'lar (arm64, armv7) va Google Play uchun AAB.
#   scripts/android-release.sh
# Natija: dist/android/<versiya>/ — KHOFace-<versiya>-arm64.apk, …-armv7.apk, ….aab, SHA256SUMS.
#
# Talab: gen/android/keystore.properties (scripts/android-keystore.sh). Faqat telefonda sinash uchun
# kalitsiz: KHOFACE_DEBUG_SIGN=1 scripts/android-release.sh (debug kalit — tarqatmang!).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FRONT="$ROOT/frontend"
ANDROID="$FRONT/src-tauri/gen/android"
VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$FRONT/src-tauri/tauri.conf.json")"
SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
APKSIGNER="$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)/apksigner"

if [[ "${KHOFACE_DEBUG_SIGN:-}" == "1" ]]; then
  echo "!! DEBUG KALIT bilan imzolanmoqda — faqat sinov uchun, tarqatmang" >&2
  SUFFIX="-debugsigned"
elif [[ ! -f "$ANDROID/keystore.properties" ]]; then
  echo "keystore.properties yo'q: avval scripts/android-keystore.sh ni ishga tushiring" >&2
  exit 1
else
  SUFFIX=""
fi
[[ -f "$FRONT/src-tauri/resources/models/face_detection_yunet_2023mar.onnx" ]] || {
  echo "modellar yo'q: scripts/fetch-models.sh" >&2; exit 1; }

# Oldingi build'lardan qolgan .so havolalari (masalan, x86_64) APK'ga tushib qolmasin
rm -f "$ANDROID"/app/src/main/jniLibs/*/*.so

echo "== testlar"
(cd "$FRONT" && pnpm test >/dev/null)
(cd "$FRONT/src-tauri" && cargo test --lib -q >/dev/null)

echo "== build $VERSION (arm64, armv7)"
# APK — ABI bo'yicha (kichikroq); AAB — bitta, ikkala ABI bilan (Play o'zi ajratadi).
# Ikkinchi chaqiruvda Rust qayta kompilyatsiya qilinmaydi.
(cd "$FRONT" && pnpm tauri android build --apk --split-per-abi --target aarch64 armv7)
(cd "$FRONT" && pnpm tauri android build --aab --target aarch64 armv7)

OUT="$ROOT/dist/android/$VERSION"
rm -rf "$OUT" && mkdir -p "$OUT"
APKS="$ANDROID/app/build/outputs/apk"
AABS="$ANDROID/app/build/outputs/bundle"
cp "$APKS/arm64/release/app-arm64-release.apk" "$OUT/KHOFace-$VERSION-arm64$SUFFIX.apk"
cp "$APKS/arm/release/app-arm-release.apk" "$OUT/KHOFace-$VERSION-armv7$SUFFIX.apk"
cp "$AABS/universalRelease/app-universal-release.aab" "$OUT/KHOFace-$VERSION$SUFFIX.aab"

echo "== imzo tekshiruvi"
for apk in "$OUT"/*.apk; do
  "$APKSIGNER" verify --print-certs "$apk" | grep -m1 "SHA-256 digest" | sed "s|^|$(basename "$apk"): |"
done
(cd "$OUT" && sha256sum -- * > SHA256SUMS)
echo "== tayyor: $OUT"
ls -lh "$OUT"
