/*
 * Building blocks of the settings page: the full-screen dialog page, a collapsible section, a switch row, a chip choice and the dim-curve labels.
 * Owns layout and look only; the settings themselves and what they do belong to SettingsSheet.
 */
package com.uscreen.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/**
 * Full-screen page in a dialog window. It replaces the modal bottom sheet, whose drag-to-dismiss
 * gesture competed with scrolling the list; a dialog has no draggable surface, so a drag always scrolls.
 * Back and the close button dismiss it.
 */
@Composable
internal fun FullScreenPage(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false)
    ) {
        Surface(modifier = Modifier.fillMaxSize(), color = SurfaceLow) {
            Box(modifier = Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
                content()
                IconButton(onClick = onDismiss, modifier = Modifier.align(Alignment.TopEnd)) {
                    Icon(Icons.Default.Close, contentDescription = "Close", tint = OnSurface)
                }
            }
        }
    }
}

/**
 * A collapsible card: a tappable header (icon, title, chevron) over content that expands in place.
 * [startExpanded] only sets the first state; the user's own toggling is kept while the page is open.
 */
@Composable
internal fun SettingsSection(
    title: String,
    icon: ImageVector,
    startExpanded: Boolean = false,
    content: @Composable () -> Unit,
) {
    var expanded by remember { mutableStateOf(startExpanded) }
    Surface(
        color = SurfaceMid,
        shape = RoundedCornerShape(12.dp),
        modifier = Modifier.fillMaxWidth()
    ) {
        Column {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .clickable { expanded = !expanded }
                    .padding(horizontal = 16.dp, vertical = 14.dp)
            ) {
                Icon(icon, contentDescription = null, tint = AccentSoft, modifier = Modifier.padding(end = 12.dp))
                Text(
                    title,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = OnSurface,
                    modifier = Modifier.weight(1f)
                )
                Icon(
                    if (expanded) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown,
                    contentDescription = if (expanded) "Collapse $title" else "Expand $title",
                    tint = Subtle
                )
            }
            AnimatedVisibility(
                visible = expanded,
                enter = fadeIn() + expandVertically(),
                exit = fadeOut() + shrinkVertically()
            ) {
                Column(modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp)) { content() }
            }
        }
    }
}

/** A label, a one-line explanation and a switch. */
@Composable
internal fun SwitchRow(
    title: String,
    description: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    enabled: Boolean = true,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().padding(top = 12.dp)
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(title, fontSize = 14.sp, color = if (enabled) OnSurface else Subtle)
            Text(description, fontSize = 11.sp, color = if (enabled) Muted else Subtle)
        }
        Spacer(Modifier.width(12.dp))
        Switch(checked = checked, onCheckedChange = onChange, enabled = enabled, colors = SwitchDefaults.colors())
    }
}

/** A row of mutually exclusive chips with a caption. */
@Composable
internal fun <T> ChipChoice(title: String, options: List<Pair<T, String>>, selected: T, onSelect: (T) -> Unit) {
    Text(title, fontSize = 14.sp, color = OnSurface, modifier = Modifier.padding(top = 12.dp))
    Spacer(Modifier.height(8.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEach { (value, label) ->
            FilterChip(
                selected = selected == value,
                onClick = { onSelect(value) },
                label = { Text(label) },
                colors = FilterChipDefaults.filterChipColors()
            )
        }
    }
}

private val curveLabels = mapOf(
    "linear" to "Linear",
    "ease_in" to "Ease in",
    "ease_out" to "Ease out",
    "ease_in_out" to "Ease in-out",
    "exponential" to "Exponential",
)

/**
 * Display label of a dim curve wire name; a `bezier:` curve is shown as 'Custom curve' and unknown names as they are.
 */
internal fun curveText(wire: String) = curveLabels[wire] ?: if (wire.startsWith("bezier:")) "Custom curve" else wire
