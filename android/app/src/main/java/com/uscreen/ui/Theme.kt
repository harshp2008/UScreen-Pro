package com.uscreen.ui

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

// MD3 baseline dark scheme (seed #6750A4), identical to the desktop app: one accent, tonal surfaces,
// no gradients, glows or elevation shadows anywhere in the UI.
val Accent = Color(0xFF4F378B)        // primary container: filled buttons, selected chips
val AccentSoft = Color(0xFFD0BCFF)    // primary: highlights, links, switches, sliders
val Ok = Color(0xFF8FD6A3)
val Warn = Color(0xFFF2C46D)
val ErrorTone = Color(0xFFF2B8B5)

val Surface0 = Color(0xFF141218)      // screen background
val SurfaceLow = Color(0xFF1D1B20)    // sheets, cards
val SurfaceMid = Color(0xFF211F26)    // inputs
val SurfaceHigh = Color(0xFF2B2930)   // buttons, chips
val OnSurface = Color(0xFFE6E1E5)     // primary text
val Muted = Color(0xFFCAC4D0)         // on-surface-variant: secondary text
val Subtle = Color(0xFF938F99)        // outline

private val UScreenColors = darkColorScheme(
    primary = AccentSoft,
    onPrimary = Color(0xFF381E72),
    primaryContainer = Accent,
    onPrimaryContainer = Color(0xFFEADDFF),
    secondary = Color(0xFFCCC2DC),
    onSecondary = Color(0xFF332D41),
    secondaryContainer = Color(0xFF4A4458),
    onSecondaryContainer = Color(0xFFE8DEF8),
    background = Surface0,
    onBackground = OnSurface,
    surface = Surface0,
    onSurface = OnSurface,
    surfaceVariant = Color(0xFF49454F),
    onSurfaceVariant = Muted,
    surfaceContainerLowest = Color(0xFF0F0D13),
    surfaceContainerLow = SurfaceLow,
    surfaceContainer = SurfaceMid,
    surfaceContainerHigh = SurfaceHigh,
    surfaceContainerHighest = Color(0xFF36343B),
    outline = Subtle,
    outlineVariant = Color(0xFF49454F),
    error = ErrorTone,
)

// Restrained utility shapes: 8dp cards, 4dp fields. Buttons use their own morphing pill shape.
private val UScreenShapes = Shapes(
    extraSmall = androidx.compose.foundation.shape.RoundedCornerShape(4.dp),
    small = androidx.compose.foundation.shape.RoundedCornerShape(8.dp),
    medium = androidx.compose.foundation.shape.RoundedCornerShape(12.dp),
)

/**
 * App theme. The expressive motion scheme (bouncy spatial springs) drives the onboarding wizard
 * and other major state changes; dense lists opt back into [CalmMotion].
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun UScreenTheme(content: @Composable () -> Unit) {
    MaterialExpressiveTheme(
        colorScheme = UScreenColors,
        motionScheme = MotionScheme.expressive(),
        shapes = UScreenShapes,
        content = content,
    )
}

/**
 * Standard motion scheme for dense list rows (settings, status items): same colors and shapes,
 * but critically-damped springs so rows settle quietly instead of bouncing.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun CalmMotion(content: @Composable () -> Unit) {
    MaterialExpressiveTheme(
        colorScheme = MaterialTheme.colorScheme,
        motionScheme = MotionScheme.standard(),
        shapes = MaterialTheme.shapes,
        typography = MaterialTheme.typography,
        content = content,
    )
}
