import java.util.Properties
import java.io.ByteArrayOutputStream

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Release signing: keystore.properties is NOT committed — each maintainer
// generates their own (see README "Building a release APK").
val keystoreProps = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}

fun queryGitHash(): String {
    return try {
        val stdout = ByteArrayOutputStream()
        project.exec {
            commandLine("git", "rev-parse", "--short", "HEAD")
            standardOutput = stdout
        }
        stdout.toString().trim()
    } catch (e: Exception) {
        "unknown"
    }
}

fun queryIsGitDirty(): Boolean {
    return try {
        val stdout = ByteArrayOutputStream()
        project.exec {
            commandLine("git", "status", "--porcelain")
            standardOutput = stdout
        }
        stdout.toString().trim().isNotEmpty()
    } catch (e: Exception) {
        false
    }
}

val envVersion = System.getenv("GIT_VERSION_STRING")?.trim()?.takeIf { it.isNotEmpty() }
val gitHash = queryGitHash()
val isDirty = queryIsGitDirty()
val gitVersionString = envVersion ?: if (isDirty) {
    logger.warn("WARNING: Building APK with uncommitted changes. Version will be marked as dirty.")
    "$gitHash-dirty"
} else {
    gitHash
}

android {
    namespace = "com.uscreen"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.uscreen"
        minSdk = 27
        targetSdk = 34
        versionCode = 22
        versionName = gitVersionString
        buildConfigField("String", "GIT_VERSION_STRING", "\"$gitVersionString\"")
    }

    if (keystoreProps.isNotEmpty()) {
        signingConfigs {
            create("release") {
                storeFile = rootProject.file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            if (keystoreProps.isNotEmpty()) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.5"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    // 2024.02.02 ships material3 1.2.1 built against compose 1.6.x — the
    // 2024.01.00 BOM paired material3 1.1.2 with animation-core 1.6.0, which
    // crashes with NoSuchMethodError in CircularProgressIndicator.
    implementation(platform("androidx.compose:compose-bom:2024.02.02"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.activity:activity-compose:1.8.2")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.7.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.7.0")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2")
}
