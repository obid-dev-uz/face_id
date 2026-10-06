# KHOFace: Android sinov matritsasi

Har bir relizdan oldin to'ldiriladi. Avtomatik testlar quyida keltirilgan. Qo'lda bajariladigan tekshiruvlar jadvalda.

## Avtomatik testlar

| Buyruq | Nima tekshiriladi |
|---|---|
| `cd frontend && pnpm test` | Frontend (Vitest, `@tauri-apps/api/mocks` bilan) |
| `cd frontend/src-tauri && cargo test --lib` | Rust unit testlari va golden testi (kompyuterda) |
| `scripts/android-test.sh aarch64` | Xuddi shu Rust testlari, ulangan telefonda (arm64) |
| `scripts/android-test.sh armv7` | Xuddi shu Rust testlari, 32-bit rejimda (arm64 telefon ham yuritadi) |
| `ANDROID_SERIAL=emulator-5554 scripts/android-test.sh x86_64` | API 24 emulatori (`khoface_api24`) |

**Golden testi** (`src/face/golden.rs`) tekislash natijasining bit-darajada bir xilligini va embedding mosligini (cos > 0.999) tekshiradi. Solishtirish kompyuterda yozib olingan `tests/fixtures/golden.json` bilan bajariladi. `tests/fixtures/` papkasiga qo'shilgan `.jpg` va `.png` rasmlar uchun detektor natijasi ham solishtiriladi. Golden faylni qayta yozish faqat model yoki konveyer ataylab o'zgartirilganda kerak: `KHOFACE_WRITE_GOLDEN=1 cargo test --lib golden`.

**Aniqlik benchmarki** (`src/face/bench.rs`): `KHOFACE_DATASET=<papka> cargo test --lib roc -- --ignored --nocapture`.

### Oxirgi natijalar (2026-10-06)

| Qurilma | ABI | Rust testlari | Golden |
|---|---|---|---|
| Kompyuter (Linux) | x86_64 | 46/46 | yozib olingan |
| Redmi 5 Plus, Android 8.1 | arm64-v8a | 46/46 | cos = 1.000000 |
| Redmi 5 Plus, Android 8.1 | armeabi-v7a | 46/46 | cos = 1.000000 |
| Emulator, Android 7.0 (API 24) | x86_64 | 46/46 | cos = 1.000000 |

## Qo'lda tekshiriladigan qurilmalar

| Qurilma | Nima tekshiriladi | Holat |
|---|---|---|
| Redmi 5 Plus, Android 8.1, WebView 71, 2.8 GB, Camera2 LEGACY | Barcha ssenariylar, tezlik, xotira, qizish | asosiy test qurilmasi |
| Emulator API 24, WebView 53 | Legacy bundle, UI to'g'ri chiqishi, WebView ogohlantirishi | |
| Android 7.x, armv7, ≤ 2 GB RAM | Ishga tushish, 320×320 detektor, xotira | qurilma yo'q |
| Android 10 / 14 / 15+ | Ruxsatlar, edge-to-edge, orqa kamera | qurilma yoki emulator kerak |

## Har bir qurilmada tekshiriladigan ssenariylar

- [ ] O'rnatish va birinchi ishga tushish; engine yuklanadi (Sozlamalar → diagnostika)
- [ ] Kamera ruxsati: rad etish, "Sozlamalarni ochish", qayta berish
- [ ] Yuz qo'shish: to'liq halqa, namunalar saqlanadi
- [ ] Live: tanish, FPS va kechikish (diagnostika qatori), Camera2 darajasi
- [ ] Old va orqa kamerani almashtirish; ilovani fonga o'tkazib qaytarish
- [ ] Jurnal: yozuvlar, snapshot, o'chirish
- [ ] Shaxs: tahrirlash, faolsizlantirish, o'chirish
- [ ] Orqaga tugmasi; ekranni burish
- [ ] Til almashtirish (uz / ru / en)
- [ ] 10 daqiqa Live: qizish, xotira (`adb shell dumpsys meminfo kho.face`)
- [ ] Zaxira nusxa; barcha ma'lumotlarni o'chirish
