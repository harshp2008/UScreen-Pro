package com.uscreen.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uscreen.HostConfig
import com.uscreen.SamsungSPen
import com.uscreen.StylusActions
import com.uscreen.StylusCatalog
import com.uscreen.StylusProfileInfo
import com.uscreen.TouchCapture

/** Marks the "Custom..." entry of the action menu. */
private const val CUSTOM = "\u0000custom"

/**
 * Settings section content for stylus profiles: pick the active one, and create, edit, test or
 * delete custom ones. The profiles live on the computer (`~/.config/uscreen/stylus_profiles/`) and
 * are the same ones the desktop app's Stylus Profile Manager edits; this only talks to the host
 * through [touchCapture].
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun StylusProfileManager(
    hostConfig: HostConfig,
    touchCapture: TouchCapture?,
    onSetting: (key: String, value: Any) -> Unit,
) {
    var catalog by remember { mutableStateOf(touchCapture?.stylusCatalog ?: StylusCatalog()) }
    var status by remember { mutableStateOf("") }
    var editing by remember { mutableStateOf<StylusProfileInfo?>(null) }
    var creating by remember { mutableStateOf(false) }
    // Id to switch to once the host confirms it saved the profile.
    var activateOnSave by remember { mutableStateOf<String?>(null) }

    DisposableEffect(touchCapture) {
        touchCapture?.onStylusProfiles = { catalog = it }
        touchCapture?.onStylusResult = { op, ok, message ->
            status = when {
                !ok -> "Failed: $message"
                op == "test" -> "Ran $message on the computer"
                op == "save" -> "Saved"
                else -> "Deleted"
            }
            if (ok && op == "save") activateOnSave?.let { onSetting("stylus_profile", it); activateOnSave = null }
        }
        onDispose {
            touchCapture?.onStylusProfiles = null
            touchCapture?.onStylusResult = null
        }
    }

    val samsung = SamsungSPen.isSupportedDevice()
    // The Samsung profile is only offered on a Samsung tablet (or when it is already the active one).
    val visible = catalog.profiles.filter { it.id != "samsung_spen" || samsung || hostConfig.stylusProfile == it.id }
    val active = catalog.profiles.firstOrNull { it.id == hostConfig.stylusProfile }

    Text("Active profile", fontSize = 14.sp, color = OnSurface, modifier = Modifier.padding(top = 12.dp))
    Spacer(Modifier.height(8.dp))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        visible.forEach { p ->
            FilterChip(
                selected = hostConfig.stylusProfile == p.id,
                onClick = { onSetting("stylus_profile", p.id) },
                label = { Text(p.name) },
                colors = FilterChipDefaults.filterChipColors()
            )
        }
    }
    Text(
        when (hostConfig.stylusProfile) {
            "samsung_spen" -> "S Pen Remote: tap the air button or swipe in the air to step through slides. Needs the Samsung S Pen."
            "standard" -> "Any stylus: the side button reaches applications as the stylus button, exactly as before."
            else -> "Custom profile: ${active?.mappings?.size ?: 0} mapped event(s)."
        },
        fontSize = 11.sp,
        color = Muted,
        modifier = Modifier.padding(top = 8.dp)
    )

    if (hostConfig.stylusProfile == "samsung_spen" && SamsungSPen.remoteUnsupported) {
        Text(
            "This tablet's S Pen has no Air Actions (Bluetooth), so there are no air button or air swipe events. " +
                "Only the side button works; it behaves exactly as in the Standard profile.",
            fontSize = 11.sp,
            color = Muted,
            modifier = Modifier.padding(top = 8.dp)
        )
    }

    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 12.dp)) {
        Button(onClick = { creating = true }, enabled = touchCapture?.isControlConnected() == true) { Text("New profile") }
        if (active != null && !active.builtin) {
            OutlinedButton(onClick = { editing = active }) { Text("Edit") }
            OutlinedButton(onClick = {
                touchCapture?.deleteStylusProfile(active.id)
                onSetting("stylus_profile", "standard")
            }) { Text("Delete") }
        }
    }
    if (status.isNotEmpty()) Text(status, fontSize = 11.sp, color = AccentSoft, modifier = Modifier.padding(top = 8.dp))

    if (creating || editing != null) {
        ProfileEditor(
            catalog = catalog,
            existing = editing,
            onTest = { touchCapture?.testStylusAction(it) },
            onSave = { id, name, mappings ->
                activateOnSave = id
                touchCapture?.saveStylusProfile(id, name, mappings)
                creating = false
                editing = null
            },
            onDismiss = { creating = false; editing = null }
        )
    }
}

/** Lower-case slug of a profile name, made unique among the existing ids. */
private fun idFor(name: String, taken: Set<String>): String {
    val base = name.lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-').take(40).ifEmpty { "profile" }
    var id = base
    var n = 2
    while (id in taken || id == "standard" || id == "samsung_spen") id = "$base-${n++}"
    return id
}

@Composable
private fun ProfileEditor(
    catalog: StylusCatalog,
    existing: StylusProfileInfo?,
    onTest: (String) -> Unit,
    onSave: (id: String, name: String, mappings: Map<String, String>) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf(existing?.name ?: "") }
    val actions = remember { mutableStateMapOf<String, String>().apply { existing?.mappings?.let { putAll(it) } } }
    val triggers = catalog.triggers
    val invalid = actions.any { (_, v) -> !StylusActions.isValid(v) }

    FullScreenPage(onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 8.dp)
        ) {
            Text(
                if (existing == null) "New stylus profile" else "Edit stylus profile",
                fontSize = 20.sp, fontWeight = FontWeight.Bold, color = OnSurface
            )
            Spacer(Modifier.height(12.dp))
            OutlinedTextField(
                value = name,
                onValueChange = { name = it.take(60) },
                label = { Text("Profile name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth()
            )
            Spacer(Modifier.height(16.dp))
            Text("When this happens, do this", fontSize = 14.sp, color = Muted)
            triggers.forEach { t ->
                MappingRow(
                    label = t.label,
                    value = actions[t.key].orEmpty(),
                    onChange = { if (it.isEmpty()) actions.remove(t.key) else actions[t.key] = it },
                    onTest = onTest,
                )
            }
            Spacer(Modifier.height(20.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    enabled = name.isNotBlank() && !invalid,
                    onClick = {
                        val id = existing?.id ?: idFor(name, catalog.profiles.map { it.id }.toSet())
                        onSave(id, name.trim(), actions.filterValues { it.isNotBlank() })
                    }
                ) { Text("Save profile") }
                TextButton(onClick = onDismiss) { Text("Cancel") }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** One event with its action: a menu of presets plus "Custom...", and a Test button that runs it on the computer. */
@Composable
private fun MappingRow(label: String, value: String, onChange: (String) -> Unit, onTest: (String) -> Unit) {
    var menu by remember { mutableStateOf(false) }
    val isPreset = StylusActions.PRESETS.any { it.first == value }
    var custom by remember { mutableStateOf(!isPreset) }
    Column(modifier = Modifier.fillMaxWidth().padding(top = 12.dp)) {
        Text(label, fontSize = 14.sp, color = OnSurface)
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 4.dp)) {
            Column(modifier = Modifier.weight(1f)) {
                OutlinedButton(onClick = { menu = true }, modifier = Modifier.fillMaxWidth()) {
                    Text(if (custom) "Custom: ${value.ifEmpty { "..." }}" else StylusActions.label(value), maxLines = 1)
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    StylusActions.PRESETS.forEach { (action, text) ->
                        DropdownMenuItem(text = { Text(text) }, onClick = {
                            custom = false
                            onChange(action)
                            menu = false
                        })
                    }
                    DropdownMenuItem(text = { Text("Custom...") }, onClick = {
                        custom = true
                        menu = false
                    })
                }
            }
            Spacer(Modifier.width(8.dp))
            OutlinedButton(onClick = { onTest(value) }, enabled = value.isNotBlank() && StylusActions.isValid(value)) {
                Text("Test")
            }
        }
        if (custom) {
            OutlinedTextField(
                value = value,
                onValueChange = onChange,
                label = { Text("Key combination, e.g. ctrl+alt+t") },
                isError = !StylusActions.isValid(value),
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp)
            )
        }
    }
}
