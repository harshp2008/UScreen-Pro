/*
 * A button shape that morphs between a pill and a rounded square while pressed
 * (androidx.graphics.shapes).
 */
package com.uscreen.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.RowScope
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MotionScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asComposePath
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.CornerRounding
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import androidx.graphics.shapes.rectangle
import androidx.graphics.shapes.toPath
import androidx.compose.runtime.getValue

/**
 * A button outline that is a real [Morph] between two [RoundedPolygon]s (androidx.graphics.shapes):
 * a full pill at rest and a tighter rounded square when pressed. [progress] is 0 (pill) .. 1 (pressed).
 * The polygons are rebuilt per measured size so the pill always has a half-height radius.
 */
class PillToSquareShape(private val progress: Float) : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline {
        val w = size.width
        val h = size.height
        if (w <= 0f || h <= 0f) return Outline.Rectangle(androidx.compose.ui.geometry.Rect(0f, 0f, w, h))
        val pill = RoundedPolygon.rectangle(
            width = w, height = h,
            rounding = CornerRounding(radius = h / 2f),
            centerX = w / 2f, centerY = h / 2f,
        )
        val pressed = RoundedPolygon.rectangle(
            width = w, height = h,
            rounding = CornerRounding(radius = h * 0.22f),
            centerX = w / 2f, centerY = h / 2f,
        )
        val path = Morph(pill, pressed).toPath(progress.coerceIn(0f, 1f)).asComposePath()
        return Outline.Generic(path)
    }
}

/**
 * Primary action button (Connect / Pair / Next / Open settings...). Pill at rest; on press it springs
 * to a rounded square and back, using the theme's expressive fast-spatial spring.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MorphButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: ButtonColors = ButtonDefaults.buttonColors(),
    contentPadding: PaddingValues = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
    content: @Composable RowScope.() -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    // Always the expressive spring, even inside a CalmMotion list: the primary action is the one
    // element that should feel springy.
    val spec = remember { MotionScheme.expressive().fastSpatialSpec<Float>() }
    val progress by animateFloatAsState(
        targetValue = if (pressed) 1f else 0f,
        animationSpec = spec,
        label = "MorphButtonShape",
    )
    Button(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        shape = PillToSquareShape(progress),
        colors = colors,
        contentPadding = contentPadding,
        interactionSource = interaction,
        content = content,
    )
}
