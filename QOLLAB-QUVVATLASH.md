# KHOFace: qo'llab-quvvatlash siyosati

Bu hujjat qaysi tizim versiyalari rasman qo'llab-quvvatlanishini, qaysilari "best-effort" asosida ishlashini va qachon to'xtatilishini belgilaydi.

## Android (ustuvor platforma)

| Daraja | Versiya | Izoh |
|---|---|---|
| **Rasmiy** | Android 8.0+ (API 26+), arm64-v8a | Har bir reliz shu versiyalarda sinaladi. Asosiy test qurilmasi: Redmi 5 Plus (Android 8.1). |
| **Rasmiy** | Android 7.0–7.1 (API 24–25) | Jackson 2.13.5 va legacy JS bundle bilan ishlaydi. API 24 emulatorida sinalgan. |
| **Best-effort** | armeabi-v7a (32-bit ARM) | Build yig'iladi va arm64 qurilmada 32-bit rejimda sinalgan. Tahlil 64-bitdagiga qaraganda sekinroq. |
| **Best-effort** | RAM ≤ 2 GB | Rasm detektori avtomatik ravishda 320×320 o'lchamga tushadi. `largeHeap` ishlatilmaydi. |
| Qo'llab-quvvatlanmaydi | Android 6 va undan eski, x86 telefonlar | |

**WebView talabi:** ilova Chrome 53+ WebView'da ishlaydi. Chrome 60 dan eski WebView'da ilova foydalanuvchiga "Android System WebView"ni yangilashni tavsiya qiladi.

**Kamera:** Camera2'ning LEGACY darajasidagi qurilmalar ham qo'llab-quvvatlanadi. Bunday qurilmalarda ham kamera 640×480 o'lchamda ishlaydi. Qurilmaning Camera2 darajasi Live ekrandagi diagnostika qatorida ko'rsatiladi.

## Desktop va iOS

Hozircha kechiktirilgan. Windows 7/8.1 va eski macOS (10.13–10.14) uchun qo'llab-quvvatlash darajasi faqat best-effort bo'ladi. Ular uchun siyosat shu platformalar ustida ish boshlanganda yoziladi.

## Xatolarni tuzatish tartibi

1. **Rasmiy** versiyalardagi xatolar keyingi relizda tuzatiladi.
2. **Best-effort** versiyalardagi xatolar past ustuvorlikda tuzatiladi. Agar tuzatish rasmiy versiyalarni buzsa, u kiritilmaydi.
3. Biror versiya bo'yicha qo'llab-quvvatlash to'xtatilishi kamida bitta reliz oldin e'lon qilinadi. Eng avval to'xtatiladigan nomzodlar: Android 7 va 32-bit ARM. Ular to'xtatilsa, minSdk 26 ga ko'tariladi va Jackson pin'i olib tashlanadi.
