# KHOFace: Android'da paketlash va tarqatish

## 1. Imzo kaliti (bir marta)

```bash
scripts/android-keystore.sh
```

Skript parolni so'raydi, `~/.config/khoface/khoface-release.jks` faylini yaratadi va `frontend/src-tauri/gen/android/keystore.properties` ni yozadi. Ikkala fayl ham git'ga tushmaydi.

- **Kalit yo'qolsa:** o'rnatilgan ilovani yangilab bo'lmaydi. Foydalanuvchilar uni o'chirib, qayta o'rnatishga majbur bo'ladi va bunda qurilmadagi barcha ma'lumotlar yo'qoladi.
- **Zaxira nusxa:** keystore va parolning nusxasini repodan tashqarida, ikki xil joyda saqlang (masalan, shifrlangan USB va parol menejeri).

## 2. Release

```bash
scripts/android-release.sh
```

Skript avval testlarni ishga tushiradi, keyin `dist/android/<versiya>/` papkasiga quyidagilarni tayyorlaydi:

| Fayl | Kim uchun |
|---|---|
| `KHOFace-<v>-arm64.apk` | 64-bit telefonlar (2017 yildan keyingi deyarli barcha qurilmalar) |
| `KHOFace-<v>-armv7.apk` | 32-bit telefonlar (eski Android 7 qurilmalari) |
| `KHOFace-<v>.aab` | Google Play (Play ABI bo'yicha o'zi ajratadi) |
| `SHA256SUMS` | Yuklangan fayl buzilmaganini tekshirish uchun |

Versiya `frontend/src-tauri/tauri.conf.json` dagi `version` maydonidan olinadi. Har bir relizdan oldin uni oshiring: `versionCode` shundan hisoblanadi va Android eski versiyani yangisining ustiga o'rnatmaydi.

Kalitsiz sinov uchun: `KHOFACE_DEBUG_SIGN=1 scripts/android-release.sh`. Bunda fayllar debug kalit bilan imzolanadi va nomiga `-debugsigned` qo'shiladi. Ularni **tarqatmang**.

## 3. Tarqatish kanallari

| Kanal | Afzalligi | E'tibor berish kerak |
|---|---|---|
| **To'g'ridan-to'g'ri APK** (Telegram, sayt, USB) | Darhol tarqatish mumkin, hisob ochish shart emas. Play'siz qurilmalarda ham ishlaydi | Foydalanuvchi "Noma'lum manbalar"ga ruxsat berishi kerak. Play Protect ogohlantirish chiqarishi mumkin. Yangilanishlarni qo'lda tarqatish kerak |
| **Google Play** | Avtomatik yangilanish, foydalanuvchi ishonchi | Dasturchi hisobi (bir martalik to'lov) kerak. Yangi ilovalar uchun eng so'nggi `targetSdk` talab qilinadi (bizda 37, minSdk 24 ga xalaqit bermaydi). Maxfiylik siyosati va "Data safety" formasini to'ldirish kerak. Yuz ma'lumotlari biometrik ma'lumot hisoblanadi: ular qurilmadan chiqmasligini aniq yozish kerak. Play App Signing yoqiladi |
| **RuStore** va boshqa do'konlar | MDH'da Play'siz qurilmalar uchun | Har bir do'konning o'z moderatsiyasi bor. Bitta APK ishlatiladi |

**Tavsiya:** avval to'g'ridan-to'g'ri APK bilan boshlash (arm64 + armv7). Ilova barqaror ishlay boshlagach, Google Play'ga AAB bilan chiqish.

## 4. Yangilanish

Mobil ilova o'zini o'zi yangilamaydi. Yangilanish do'kon orqali yoki yangi APK'ni ustiga o'rnatish bilan bo'ladi. Ustiga o'rnatish faqat bir xil kalit bilan imzolangan va `versionCode` kattaroq bo'lgan APK bilan ishlaydi. Bunda ma'lumotlar saqlanib qoladi.

## CI

Loyihada hozircha git remote yo'q. Shuning uchun CI o'rniga `scripts/android-release.sh` ishlatiladi. Remote paydo bo'lganda, keystore CI'ning maxfiy o'zgaruvchilariga base64 ko'rinishida qo'yiladi va o'sha skript ishga tushiriladi.
