pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "uscreen"
include(":app")
// Clean-room, Apache-2.0 S Pen Remote SDK (vendored, see spen-remote/VENDORED.txt): same API as
// Samsung's proprietary SDK, without its JARs. It reports "unsupported" on non-Samsung devices.
include(":spen-remote")
