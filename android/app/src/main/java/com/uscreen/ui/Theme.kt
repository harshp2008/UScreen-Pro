package com.uscreen.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

// Flat tonal palette shared with the desktop app (Material 3 dark scheme).
// No gradients, glows or elevation shadows anywhere in the UI.
val Accent = Color(0xFF00497D)        // primary container: filled buttons, selected chips
val AccentSoft = Color(0xFFA8C8FF)    // primary: highlights, links
val Ok = Color(0xFF8FD6A3)
val Warn = Color(0xFFF2C46D)
val ErrorTone = Color(0xFFFFB4AB)

val Surface0 = Color(0xFF111318)      // screen background
val SurfaceLow = Color(0xFF191C20)    // sheets
val SurfaceMid = Color(0xFF1D2024)    // cards
val SurfaceHigh = Color(0xFF272A2F)   // buttons, chips
val Muted = Color(0xFFC3C6CF)         // on-surface-variant
val Subtle = Color(0xFF8D9199)        // outline

/**
 * Global dark theme applied across all UScreen Android UI screens and dialogs.
 * Material 3 colour roles with restrained shapes (8dp cards, 4dp fields).
 */
@Composable
fun UScreenTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = AccentSoft,
            onPrimary = Color(0xFF003258),
            primaryContainer = Accent,
            onPrimaryContainer = Color(0xFFD3E4FF),
            secondary = Color(0xFFBBC7DB),
            secondaryContainer = Color(0xFF3B4858),
            background = Surface0,
            surface = SurfaceLow,
            surfaceVariant = SurfaceMid,
            onSurface = Color(0xFFE2E2E9),
            onSurfaceVariant = Muted,
            outline = Subtle,
            outlineVariant = Color(0xFF43474E),
            error = ErrorTone,
        )
    ) {
        content()
    }
}
