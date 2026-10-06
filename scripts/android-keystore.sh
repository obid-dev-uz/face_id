#!/usr/bin/env bash
# Release imzo kalitini (keystore) bir marta yaratadi va gen/android/keystore.properties ni yozadi.
#   scripts/android-keystore.sh [keystore yo'li]    (standart: ~/.config/khoface/khoface-release.jks)
#
# MUHIM: kalit va parolni yo'qotsangiz, o'rnatilgan ilovani yangilab bo'lmaydi (foydalanuvchilar
# o'chirib, qayta o'rnatishi kerak bo'ladi). Keystore'ning zaxira nusxasini repo'dan tashqarida,
# xavfsiz joyda saqlang. Parol faqat keystore.properties da (git'da emas) qoladi.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROPS="$ROOT/frontend/src-tauri/gen/android/keystore.properties"
STORE="${1:-$HOME/.config/khoface/khoface-release.jks}"
ALIAS=khoface

[[ -e "$STORE" ]] && { echo "keystore allaqachon bor: $STORE (ustiga yozilmaydi)" >&2; exit 1; }
[[ -e "$PROPS" ]] && { echo "$PROPS allaqachon bor (ustiga yozilmaydi)" >&2; exit 1; }
KEYTOOL="$(command -v keytool || true)"
[[ -n "$KEYTOOL" ]] || { echo "keytool topilmadi (JDK kerak)" >&2; exit 1; }

read -r -s -p "Keystore paroli (kamida 8 belgi): " PASS; echo
read -r -s -p "Parolni takrorlang: " PASS2; echo
[[ "$PASS" == "$PASS2" ]] || { echo "parollar mos emas" >&2; exit 1; }
[[ ${#PASS} -ge 8 ]] || { echo "parol juda qisqa" >&2; exit 1; }

export PASS
mkdir -p "$(dirname "$STORE")"
# PKCS12: kalit paroli keystore paroli bilan bir xil. 30 yil amal qiladi (Play kamida 25 yil talab qiladi).
"$KEYTOOL" -genkeypair -v -storetype PKCS12 -keystore "$STORE" -alias "$ALIAS" \
  -keyalg RSA -keysize 4096 -validity 10950 -dname "CN=KHOFace, O=KHO" \
  -storepass:env PASS -keypass:env PASS
chmod 600 "$STORE"

umask 077
cat > "$PROPS" <<EOF
storeFile=$STORE
storePassword=$PASS
keyAlias=$ALIAS
keyPassword=$PASS
EOF
echo "tayyor: $STORE"
echo "        $PROPS"
echo "Keystore'ning zaxira nusxasini oling (masalan, shifrlangan USB yoki parol menejeri)."
