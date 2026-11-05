package com.uscreen.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val Accent = Color(0xFF6C63FF)
val AccentSoft = Color(0xFF8B85FF)
val Ok = Color(0xFF4CAF50)
val Warn = Color(0xFFFF9800)

/**
 * Global dark theme applied across all UScreen Android UI screens and dialogs.
 * Provides consistent typography, high-contrast dark backgrounds (optimal for OLED panels),
 * and distinctive primary accent colors.
 */
@Composable
fun UScreenTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Accent,
            secondary = Color(0xFF03DAC6),
            background = Color(0xFF0A0A0A),
            surface = Color(0xFF16161F),
            surfaceVariant = Color(0xFF20202C),
        )
    ) {
        content()
    }
}
