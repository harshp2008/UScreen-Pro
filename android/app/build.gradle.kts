import java.util.Properties
import java.io.ByteArrayOutputStream

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
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
    compileSdk = 36

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

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    // material3 is pinned to 1.5.0-alpha04: MotionScheme / MaterialExpressiveTheme are public API only from the 1.5 line (internal in 1.4.0, and the BOM maps 1.3.x).
    implementation(platform("androidx.compose:compose-bom:2025.09.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3:1.5.0-alpha04")
    // Path/shape morphing (Morph, RoundedPolygon) for the expressive Connect / Pair button.
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.activity:activity-compose:1.8.2")
    // AppCompatActivity + AppCompatDelegate.setDefaultNightMode for the Light / Dark / System theme choice.
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.7.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.7.0")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    testImplementation("junit:junit:4.13.2")
    // android.jar only has stubs for org.json; this is the real one, for JVM tests of the wire parsing.
    testImplementation("org.json:json:20231013")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2")
}
