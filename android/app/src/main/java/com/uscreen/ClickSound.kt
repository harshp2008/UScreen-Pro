/*
 * A short click sound for touchpad clicks, generated in code so no sound file is needed.
 */
package com.uscreen

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import kotlin.math.PI
import kotlin.math.exp
import kotlin.math.sin

/** Plays a roughly 12 ms decaying tick through the media stream. Safe to call from any thread; a device that cannot play it stays silent. */
object ClickSound {
    private const val SAMPLE_RATE = 44100
    private var track: AudioTrack? = null

    /** The tick: a 1.8 kHz sine that dies away quickly, as 16-bit samples. */
    private fun samples(): ShortArray {
        val n = SAMPLE_RATE * 12 / 1000
        return ShortArray(n) { i ->
            val t = i.toDouble() / SAMPLE_RATE
            (sin(2 * PI * 1800 * t) * exp(-t * 350) * 0.8 * Short.MAX_VALUE).toInt().toShort()
        }
    }

    /** One click. */
    @Synchronized
    fun play() {
        try {
            val t = track ?: run {
                val pcm = samples()
                AudioTrack.Builder()
                    .setAudioAttributes(
                        AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION)
                            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                            .build()
                    )
                    .setAudioFormat(
                        AudioFormat.Builder()
                            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                            .setSampleRate(SAMPLE_RATE)
                            .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                            .build()
                    )
                    .setTransferMode(AudioTrack.MODE_STATIC)
                    .setBufferSizeInBytes(pcm.size * 2)
                    .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                    .build()
                    .also { it.write(pcm, 0, pcm.size); track = it }
            }
            t.stop()
            t.reloadStaticData()
            t.play()
        } catch (_: Exception) {
            // A click sound is a nicety; never let it break input.
        }
    }
}
