// Top-level Gradle build of the Android app: the plugin versions (Android, Kotlin, Compose). The app module is android/app/build.gradle.kts.
plugins {
    id("com.android.application") version "8.9.3" apply false
    id("org.jetbrains.kotlin.android") version "2.1.20" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.1.20" apply false
}
