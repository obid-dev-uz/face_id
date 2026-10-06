import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// KHOFace: release imzosi. keystore.properties git'da emas (scripts/android-keystore.sh yaratadi):
//   storeFile=/mutlaq/yo'l/khoface-release.jks, storePassword=…, keyAlias=khoface, keyPassword=…
// Fayl bo'lmasa, release APK imzosiz chiqadi (o'rnatib bo'lmaydi) — scripts/android-release.sh buni tekshiradi.
val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 37
    signingConfigs {
        if (keystoreProperties.containsKey("storeFile")) {
            create("release") {
                storeFile = file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }
    namespace = "kho.face"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "kho.face"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfigs.findByName("release")?.let { signingConfig = it }
            // Faqat telefonda sinash uchun (KHOFACE_DEBUG_SIGN=1): release build debug kalit bilan.
            // Bunday APK'ni tarqatmang — yangilanishlar haqiqiy kalit bilan o'rnatilmaydi.
            if (System.getenv("KHOFACE_DEBUG_SIGN") == "1") {
                signingConfig = signingConfigs.getByName("debug")
            }
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

// KHOFace: Android 7 (API 24/25) qo'llab-quvvatlash.
// Jackson 2.14+ Android 8+ da paydo bo'lgan java.lang.BootstrapMethodError klassiga murojaat qiladi,
// natijada ilova Android 7 da ishga tushishda yopiladi (tauri-apps/tauri#8788).
// Tauri 2.12 jackson 2.15.3 ni ishlatadi; uni Android 7 bilan mos 2.13.5 ga majburan tushiramiz.
val legacyJacksonVersion = "2.13.5"
configurations.all {
    resolutionStrategy.eachDependency {
        if (requested.group.startsWith("com.fasterxml.jackson")) {
            useVersion(legacyJacksonVersion)
            because("Android 7 (API 24) uchun: tauri-apps/tauri#8788")
        }
    }
}

apply(from = file("tauri.build.gradle.kts"))
