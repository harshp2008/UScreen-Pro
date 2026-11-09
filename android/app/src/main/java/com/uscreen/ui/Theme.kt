/*
 * The app's colour palette (light and dark) and Material theme, matching the desktop GUI.
 */
package com.uscreen.ui

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.foundation.isSystemInDarkTheme
import com.uscreen.Prefs
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.Composable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.dp

/**
 * The app's colors, for both schemes. Dark is token for token the `.dark` block of
 * uscreen-pro-gui/src/index.css (oklch converted to sRGB); light is the `:root` block. Same primary,
 * same tonal surfaces, no gradients, glows or elevation shadows.
 */
@Immutable
class UScreenPalette(
    val dark: Boolean,
    val accent: Color,       // --accent / primary container
    val accentSoft: Color,   // --primary: filled buttons, links, switches, sliders
    val ok: Color,           // --success
    val warn: Color,         // --warning
    val errorTone: Color,    // --destructive
    val surface0: Color,     // --background: screen
    val surfaceLow: Color,   // --card / --popover: sheets, cards
    val surfaceMid: Color,   // --sidebar-background: inputs
    val surfaceHigh: Color,  // card to muted midpoint: buttons, chips
    val onSurface: Color,    // --foreground: primary text
    val muted: Color,        // --muted-foreground: secondary text
    val subtle: Color,       // muted-foreground, dimmed: outline, disabled
)

val DarkPalette = UScreenPalette(
    dark = true,
    accent = Color(0xFF183F2C), accentSoft = Color(0xFF34D399),
    ok = Color(0xFF8FD6A3), warn = Color(0xFFF2C46D), errorTone = Color(0xFFFFB4AB),
    surface0 = Color(0xFF0C1513), surfaceLow = Color(0xFF141D1B), surfaceMid = Color(0xFF18211F),
    surfaceHigh = Color(0xFF232E2B), onSurface = Color(0xFFDBE5E0), muted = Color(0xFFB8CBC4),
    subtle = Color(0xFF85968F),
)

val LightPalette = UScreenPalette(
    dark = false,
    accent = Color(0xFFDCEDE3), accentSoft = Color(0xFF047857),
    ok = Color(0xFF1B6B3A), warn = Color(0xFF7A5900), errorTone = Color(0xFFBA1A1A),
    surface0 = Color(0xFFF5FBF7), surfaceLow = Color(0xFFECF6F1), surfaceMid = Color(0xFFE6F0EC),
    surfaceHigh = Color(0xFFDDEBE5), onSurface = Color(0xFF141D1B), muted = Color(0xFF3A4A45),
    subtle = Color(0xFF6B7C75),
)

val LocalPalette = staticCompositionLocalOf { DarkPalette }

// Call-site names are unchanged; they now resolve against the active scheme.
val Accent: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.accent
val AccentSoft: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.accentSoft
val Ok: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.ok
val Warn: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.warn
val ErrorTone: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.errorTone
val Surface0: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.surface0
val SurfaceLow: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.surfaceLow
val SurfaceMid: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.surfaceMid
val SurfaceHigh: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.surfaceHigh
val OnSurface: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.onSurface
val Muted: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.muted
val Subtle: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.subtle

private fun colorScheme(p: UScreenPalette) =
    if (p.dark) darkColorScheme(
        primary = p.accentSoft,
        onPrimary = Color(0xFF00382A),            // --primary-foreground
        primaryContainer = p.accent,
        onPrimaryContainer = Color(0xFFCFE9DB),   // --accent-foreground
        secondary = Color(0xFFCFE9DB),
        onSecondary = Color(0xFF0B2A1D),
        secondaryContainer = Color(0xFF2F4A3C),   // --secondary
        onSecondaryContainer = Color(0xFFCFE9DB), // --secondary-foreground
        tertiary = p.ok,
        onTertiary = Color(0xFF0B2A1D),           // --success-foreground
        background = p.surface0,
        onBackground = p.onSurface,
        surface = p.surface0,
        onSurface = p.onSurface,
        surfaceVariant = Color(0xFF3A4A45),       // --muted
        onSurfaceVariant = p.muted,
        surfaceContainerLowest = Color(0xFF0C1513),
        surfaceContainerLow = p.surfaceLow,
        surfaceContainer = p.surfaceMid,
        surfaceContainerHigh = p.surfaceHigh,
        surfaceContainerHighest = Color(0xFF3A4A45),
        outline = p.subtle,
        outlineVariant = Color(0xFF3A4A45),       // --border
        error = p.errorTone,
        onError = Color(0xFF690005),              // --destructive-foreground
    ) else lightColorScheme(
        primary = p.accentSoft,
        onPrimary = Color(0xFFFFFFFF),
        primaryContainer = p.accent,
        onPrimaryContainer = Color(0xFF0B2A1D),
        secondary = Color(0xFF0B2A1D),
        onSecondary = Color(0xFFFFFFFF),
        secondaryContainer = Color(0xFFC9EBD9),   // --secondary
        onSecondaryContainer = Color(0xFF0B2A1D),
        tertiary = p.ok,
        onTertiary = Color(0xFFFFFFFF),
        background = p.surface0,
        onBackground = p.onSurface,
        surface = p.surface0,
        onSurface = p.onSurface,
        surfaceVariant = Color(0xFFD4E7E0),       // --muted
        onSurfaceVariant = p.muted,
        surfaceContainerLowest = Color(0xFFFFFFFF),
        surfaceContainerLow = p.surfaceLow,
        surfaceContainer = p.surfaceMid,
        surfaceContainerHigh = p.surfaceHigh,
        surfaceContainerHighest = Color(0xFFD4E7E0),
        outline = p.subtle,
        outlineVariant = Color(0xFFB8CBC4),       // --border
        error = p.errorTone,
        onError = Color(0xFFFFFFFF),
    )

// Desktop radius scale: --radius 0.5rem; sm = 4, md = 6, lg = 8 (dialogs and the wizard card use 12).
private val UScreenShapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(6.dp),
    medium = RoundedCornerShape(8.dp),
    large = RoundedCornerShape(12.dp),
    extraLarge = RoundedCornerShape(12.dp),
)

// Desktop type: Roboto-class sans (Roboto Flex on the desktop, Roboto on Android), 12 px base text,
// 11 px captions, 20 px medium titles.
private val UScreenFont = FontFamily.SansSerif
private val UScreenTypography = Typography().let { t ->
    fun TextStyle.sans(size: Int, weight: FontWeight, line: Int) =
        copy(fontFamily = UScreenFont, fontSize = size.sp, fontWeight = weight, lineHeight = line.sp)
    t.copy(
        displayLarge = t.displayLarge.copy(fontFamily = UScreenFont),
        displayMedium = t.displayMedium.copy(fontFamily = UScreenFont),
        displaySmall = t.displaySmall.copy(fontFamily = UScreenFont),
        headlineLarge = t.headlineLarge.copy(fontFamily = UScreenFont),
        headlineMedium = t.headlineMedium.copy(fontFamily = UScreenFont),
        headlineSmall = t.headlineSmall.copy(fontFamily = UScreenFont),
        titleLarge = t.titleLarge.sans(20, FontWeight.Medium, 28),
        titleMedium = t.titleMedium.sans(14, FontWeight.Medium, 20),
        titleSmall = t.titleSmall.sans(12, FontWeight.Medium, 16),
        bodyLarge = t.bodyLarge.sans(14, FontWeight.Normal, 20),
        bodyMedium = t.bodyMedium.sans(12, FontWeight.Normal, 16),
        bodySmall = t.bodySmall.sans(11, FontWeight.Normal, 16),
        labelLarge = t.labelLarge.sans(12, FontWeight.Medium, 16),
        labelMedium = t.labelMedium.sans(11, FontWeight.Medium, 16),
        labelSmall = t.labelSmall.sans(10, FontWeight.Medium, 14),
    )
}

/**
 * App theme. The expressive motion scheme (bouncy spatial springs) drives the onboarding wizard
 * and other major state changes; dense lists opt back into [CalmMotion].
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun UScreenTheme(themeMode: Int = Prefs.THEME_SYSTEM, content: @Composable () -> Unit) {
    // System follows the OS (also kept in step by AppCompatDelegate, see MainActivity.applyThemeMode).
    val dark = when (themeMode) {
        Prefs.THEME_LIGHT -> false
        Prefs.THEME_DARK -> true
        else -> isSystemInDarkTheme()
    }
    val palette = if (dark) DarkPalette else LightPalette
    CompositionLocalProvider(LocalPalette provides palette) {
        MaterialExpressiveTheme(
            colorScheme = colorScheme(palette),
            motionScheme = MotionScheme.expressive(),
            shapes = UScreenShapes,
            typography = UScreenTypography,
            content = content,
        )
    }
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
