package com.uscreen.ui

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.dp

// MD3 Expressive emerald dark scheme, token for token the `.dark` block of uscreen-pro-gui/src/index.css
// (oklch converted to sRGB): same primary, same tonal surfaces, no gradients, glows or elevation shadows.
val Accent = Color(0xFF183F2C)        // desktop --accent / primary container
val AccentSoft = Color(0xFF34D399)    // desktop --primary: filled buttons, links, switches, sliders
val Ok = Color(0xFF8FD6A3)            // --success
val Warn = Color(0xFFF2C46D)          // --warning
val ErrorTone = Color(0xFFFFB4AB)     // --destructive

val Surface0 = Color(0xFF0C1513)      // --background: screen
val SurfaceLow = Color(0xFF141D1B)    // --card / --popover: sheets, cards
val SurfaceMid = Color(0xFF18211F)    // --sidebar-background: inputs
val SurfaceHigh = Color(0xFF232E2B)   // card to muted midpoint: buttons, chips
val OnSurface = Color(0xFFDBE5E0)     // --foreground: primary text
val Muted = Color(0xFFB8CBC4)         // --muted-foreground: secondary text
val Subtle = Color(0xFF85968F)        // muted-foreground, dimmed: outline, disabled

private val UScreenColors = darkColorScheme(
    primary = AccentSoft,
    onPrimary = Color(0xFF00382A),            // --primary-foreground
    primaryContainer = Accent,
    onPrimaryContainer = Color(0xFFCFE9DB),   // --accent-foreground
    secondary = Color(0xFFCFE9DB),
    onSecondary = Color(0xFF0B2A1D),
    secondaryContainer = Color(0xFF2F4A3C),   // --secondary
    onSecondaryContainer = Color(0xFFCFE9DB), // --secondary-foreground
    tertiary = Ok,
    onTertiary = Color(0xFF0B2A1D),           // --success-foreground
    background = Surface0,
    onBackground = OnSurface,
    surface = Surface0,
    onSurface = OnSurface,
    surfaceVariant = Color(0xFF3A4A45),       // --muted
    onSurfaceVariant = Muted,
    surfaceContainerLowest = Color(0xFF0C1513),
    surfaceContainerLow = SurfaceLow,
    surfaceContainer = SurfaceMid,
    surfaceContainerHigh = SurfaceHigh,
    surfaceContainerHighest = Color(0xFF3A4A45),
    outline = Subtle,
    outlineVariant = Color(0xFF3A4A45),       // --border
    error = ErrorTone,
    onError = Color(0xFF690005),              // --destructive-foreground
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
fun UScreenTheme(content: @Composable () -> Unit) {
    MaterialExpressiveTheme(
        colorScheme = UScreenColors,
        motionScheme = MotionScheme.expressive(),
        shapes = UScreenShapes,
        typography = UScreenTypography,
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
