/*
 * The two full-screen states while a tablet is connected without video: the graphics-tablet screen (blanks the OLED panel after a timeout) and the touchpad screen with its touch area.
 * Owns what is drawn in those modes; it does not own the connection or the settings.
 */
package com.uscreen.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

/**
 * Screen displayed when running in Graphics Tablet mode.
 * Shows informational guidance and blanks out the OLED screen after a timeout to conserve battery.
 */
@Composable
fun PenOnlyScreen(blankAfterSeconds: Int) {
    var blank by remember { mutableStateOf(false) }
    LaunchedEffect(blankAfterSeconds) {
        blank = false
        if (blankAfterSeconds > 0) {
            delay(blankAfterSeconds * 1000L)
            blank = true
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
        contentAlignment = Alignment.Center
    ) {
        if (!blank) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                modifier = Modifier.padding(32.dp)
            ) {
                Text(
                    "Graphics tablet mode",
                    fontSize = 20.sp,
                    color = OnSurface,
                    fontWeight = FontWeight.Bold
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    "The pen drives your computer's screen.\n" +
                        "Touch input is sent back; no video is streamed here.",
                    fontSize = 14.sp,
                    color = Muted,
                    textAlign = TextAlign.Center
                )
                if (blankAfterSeconds > 0) {
                    Spacer(Modifier.height(16.dp))
                    Text(
                        "The screen goes black in a few seconds to spare OLED panels. " +
                            "Settings are behind the gear icon in the corner.",
                        fontSize = 12.sp,
                        color = Muted,
                        textAlign = TextAlign.Center
                    )
                }
            }
        }
    }
}

/**
 * The single static screen shown while Touchpad mode is on. The host has stopped capturing and
 * encoding for this tablet, so nothing else will ever arrive to draw here.
 *
 * Outside the touch area ([shape]) the screen is pure black and touches there do nothing; the area
 * itself is a slightly lighter panel with a faint border. With [showRightClickZone] a faint line
 * across its bottom 15% and a divider mark the right-click zone (the right half of that strip).
 */
@Composable
fun TouchpadActiveScreen(
    showRightClickZone: Boolean = true,
    shape: String = com.uscreen.TouchpadShape.FULL,
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
        contentAlignment = Alignment.Center
    ) {
        TouchpadArea(shape, showRightClickZone)
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.padding(32.dp)
        ) {
            Text(
                "Touchpad Mode Active",
                fontSize = 20.sp,
                color = OnSurface,
                fontWeight = FontWeight.Bold
            )
            Spacer(Modifier.height(8.dp))
            Text(
                "Move your finger to move the pointer. Tap to click, two fingers to scroll or pinch.\n" +
                    "No video is streamed to this screen. Settings are behind the gear icon.",
                fontSize = 14.sp,
                color = Muted,
                textAlign = TextAlign.Center
            )
        }
    }
}

/** Draws the touch area for [shape] and, optionally, its right-click zone. */
@Composable
private fun TouchpadArea(shape: String, showRightClickZone: Boolean) {
    val line = Color(0x33FFFFFF)
    Box(modifier = Modifier.fillMaxSize()) {
        androidx.compose.foundation.Canvas(modifier = Modifier.fillMaxSize()) {
            val b = com.uscreen.TouchpadShape.bounds(shape, size.width, size.height)
            val restricted = shape != com.uscreen.TouchpadShape.FULL
            val topLeft = androidx.compose.ui.geometry.Offset(b.left, b.top)
            val area = androidx.compose.ui.geometry.Size(b.width, b.height)
            if (restricted) {
                drawRect(Color(0xFF151515), topLeft, area)
                drawRect(line, topLeft, area, style = androidx.compose.ui.graphics.drawscope.Stroke(width = 3f))
            }
            if (showRightClickZone) {
                val y = b.bottom - b.height * com.uscreen.TouchpadShape.ZONE_FRACTION
                val mid = (b.left + b.right) / 2f
                drawLine(line, androidx.compose.ui.geometry.Offset(b.left, y), androidx.compose.ui.geometry.Offset(b.right, y), strokeWidth = 2f)
                drawLine(line, androidx.compose.ui.geometry.Offset(mid, y), androidx.compose.ui.geometry.Offset(mid, b.bottom), strokeWidth = 2f)
            }
        }
        if (showRightClickZone) {
            // Anchored to the touch area, not the screen: the label sits in its bottom-right corner.
            androidx.compose.foundation.layout.BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
                val wPx = constraints.maxWidth.toFloat()
                val hPx = constraints.maxHeight.toFloat()
                val b = com.uscreen.TouchpadShape.bounds(shape, wPx, hPx)
                val density = androidx.compose.ui.platform.LocalDensity.current
                Text(
                    "Right click",
                    fontSize = 11.sp,
                    color = Color(0x40FFFFFF),
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .padding(
                            end = with(density) { (wPx - b.right).toDp() } + 24.dp,
                            bottom = with(density) { (hPx - b.bottom).toDp() } + 12.dp,
                        )
                )
            }
        }
    }
}
