/**
 * BeatWave Project (C) 2026
 * Licensed under GPL-3.0 | See git history for contributors
 *
 * Foreground service that plays YouTube videos (separate from the music queue).
 * Holds the ExoPlayer so audio keeps playing with the screen off, exactly like
 * music. The player's audio chain reuses the app's parametric EQ processor, so
 * the user's sound-enhance settings apply to video too.
 *
 * Stream resolution: innertube player() with a video client, then
 * YouTube.newPipePlayer() to decipher every format URL. Video+audio adaptive
 * formats are served to ExoPlayer as a DASH manifest written to cache; when no
 * adaptive pair exists, the best muxed (progressive) format plays directly.
 */

package com.beatwave.music.playback

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Binder
import android.os.Build
import android.os.IBinder
import androidx.annotation.OptIn
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.Player
import androidx.media3.common.audio.SonicAudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.audio.DefaultAudioSink
import androidx.media3.exoplayer.audio.SilenceSkippingAudioProcessor
import com.beatwave.music.MainActivity
import com.beatwave.music.R
import com.beatwave.music.eq.EqualizerService
import com.beatwave.music.eq.audio.CustomEqualizerAudioProcessor
import com.beatwave.music.eq.data.EQProfileRepository
import com.music.innertube.YouTube
import com.music.innertube.models.YouTubeClient
import com.music.innertube.models.response.PlayerResponse
import dagger.hilt.android.AndroidEntryPoint
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.File
import javax.inject.Inject

data class VideoMeta(
    val videoId: String,
    val title: String,
    val thumbnailUrl: String,
)

data class VideoNowPlaying(
    val meta: VideoMeta,
    /** Available video heights, highest first. Empty when playing a muxed fallback. */
    val heights: List<Int>,
    val isDash: Boolean,
)

@OptIn(UnstableApi::class)
@AndroidEntryPoint
class VideoPlaybackService : Service() {

    @Inject
    lateinit var equalizerService: EqualizerService

    @Inject
    lateinit var eqProfileRepository: EQProfileRepository

    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    private lateinit var player: ExoPlayer
    private var eqProcessor: CustomEqualizerAudioProcessor? = null
    private var currentMeta: VideoMeta? = null
    private var currentHeights: List<Int> = emptyList()
    private var currentIsDash: Boolean = false

    private val binder = VideoBinder()

    inner class VideoBinder : Binder() {
        fun getService(): VideoPlaybackService = this@VideoPlaybackService
        fun getPlayer(): ExoPlayer = player
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        val eq = CustomEqualizerAudioProcessor()
        eqProcessor = eq
        equalizerService.addAudioProcessor(eq)
        player = ExoPlayer.Builder(this)
            .setRenderersFactory(
                object : DefaultRenderersFactory(this) {
                    override fun buildAudioSink(
                        context: Context,
                        enableFloatOutput: Boolean,
                        enableAudioTrackPlaybackParams: Boolean,
                    ) = DefaultAudioSink.Builder(this@VideoPlaybackService)
                        .setEnableFloatOutput(enableFloatOutput)
                        .setEnableAudioTrackPlaybackParams(enableAudioTrackPlaybackParams)
                        .setAudioProcessorChain(
                            DefaultAudioSink.DefaultAudioProcessorChain(
                                arrayOf(eq),
                                SilenceSkippingAudioProcessor(2_000_000, 20_000, 256),
                                SonicAudioProcessor(),
                            ),
                        ).build()
                },
            )
            .setHandleAudioBecomingNoisy(true)
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MOVIE)
                    .build(),
                false,
            )
            .build()
        player.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                updateNotification()
            }

            override fun onPlaybackStateChanged(state: Int) {
                if (state == Player.STATE_ENDED) {
                    stopPlayback()
                }
            }
        })

        // Same EQ wiring as MusicService: video gets the active profile (the
        // built-in "Averon Enhanced" by default) even when MusicService never ran
        // in this process — which is the normal case now that Video is the
        // landing tab. Profile changes apply to video too, live.
        serviceScope.launch {
            eqProfileRepository.activeProfile.collect { profile ->
                if (profile != null) {
                    equalizerService.applyProfile(profile)
                } else {
                    equalizerService.disable()
                }
                if (player.isPlaying) {
                    player.seekTo(player.currentPosition)
                }
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_TOGGLE -> {
                if (player.isPlaying) player.pause() else player.play()
                updateNotification()
            }
            ACTION_STOP -> stopPlayback()
            ACTION_PLAY -> {
                val videoId = intent.getStringExtra(EXTRA_VIDEO_ID) ?: return START_STICKY
                val title = intent.getStringExtra(EXTRA_TITLE) ?: ""
                val thumb = intent.getStringExtra(EXTRA_THUMB) ?: ""
                playVideo(VideoMeta(videoId, title, thumb))
            }
        }
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onDestroy() {
        eqProcessor?.let { equalizerService.removeAudioProcessor(it) }
        player.release()
        serviceScope.cancel()
        super.onDestroy()
    }

    fun playVideo(meta: VideoMeta) {
        if (currentMeta?.videoId == meta.videoId && player.playbackState != Player.STATE_IDLE) {
            player.play()
            return
        }
        currentMeta = meta
        serviceScope.launch(Dispatchers.IO) {
            val resolved = resolveStreams(meta.videoId)
            launch(Dispatchers.Main) {
                if (resolved == null) {
                    _error.value = "Could not load this video"
                    return@launch
                }
                _error.value = null
                val item = when (resolved) {
                    is Resolved.Dash -> {
                        val file = File(cacheDir, "video-${meta.videoId}.mpd")
                        file.writeText(resolved.mpd)
                        currentHeights = resolved.heights
                        currentIsDash = true
                        MediaItem.Builder()
                            .setUri(Uri.fromFile(file))
                            .setMimeType(MimeTypes.APPLICATION_MPD)
                            .build()
                    }
                    is Resolved.Muxed -> {
                        currentHeights = emptyList()
                        currentIsDash = false
                        MediaItem.Builder()
                            .setUri(resolved.url)
                            .setMimeType(resolved.mimeType)
                            .build()
                    }
                }
                player.setMediaItem(item)
                player.prepare()
                player.play()
                _nowPlaying.value = VideoNowPlaying(meta, currentHeights, currentIsDash)
                startForegroundService()
            }
        }
    }

    /** Null = Auto (ExoPlayer picks the highest playable quality). */
    fun setQuality(height: Int?) {
        player.trackSelectionParameters = player.trackSelectionParameters
            .buildUpon()
            .setMaxVideoSize(Int.MAX_VALUE, height ?: Int.MAX_VALUE)
            .build()
        _selectedHeight.value = height
    }

    fun toggle() {
        if (player.isPlaying) player.pause() else player.play()
    }

    fun stopPlayback() {
        player.stop()
        player.clearMediaItems()
        _nowPlaying.value = null
        _selectedHeight.value = null
        currentMeta = null
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    // ---- stream resolution ----

    private sealed interface Resolved {
        data class Dash(val mpd: String, val heights: List<Int>) : Resolved
        data class Muxed(val url: String, val mimeType: String) : Resolved
    }

    private suspend fun resolveStreams(videoId: String): Resolved? {
        val clients = listOf(
            YouTubeClient.TVHTML5_SIMPLY_EMBEDDED_PLAYER,
            YouTubeClient.IOS,
            YouTubeClient.TVHTML5,
        )
        for (client in clients) {
            val response = YouTube.player(videoId, null, client).getOrNull() ?: continue
            if (response.playabilityStatus.status != "OK") continue
            val resolved = YouTube.newPipePlayer(videoId, response) ?: response
            val streaming = resolved.streamingData ?: continue
            val videos = streaming.adaptiveFormats.filter {
                it.height != null && !it.url.isNullOrEmpty()
            }
            val audios = streaming.adaptiveFormats.filter {
                it.width == null && !it.url.isNullOrEmpty()
            }
            if (videos.isNotEmpty() && audios.isNotEmpty()) {
                val durationSecs = resolved.videoDetails?.lengthSeconds?.toLongOrNull()
                    ?: videos.firstNotNullOfOrNull { it.approxDurationMs?.toLongOrNull() }?.div(1000)
                    ?: 0L
                val mpd = buildDashMpd(durationSecs, videos, audios)
                val heights = videos.mapNotNull { it.height }.distinct().sortedDescending()
                return Resolved.Dash(mpd, heights)
            }
            val muxed = streaming.formats?.firstOrNull { !it.url.isNullOrEmpty() }
            val muxedUrl = muxed?.url
            if (muxedUrl != null) {
                return Resolved.Muxed(muxedUrl, muxed.mimeType.substringBefore(";"))
            }
        }
        return null
    }

    private fun buildDashMpd(
        durationSecs: Long,
        videos: List<PlayerResponse.StreamingData.Format>,
        audios: List<PlayerResponse.StreamingData.Format>,
    ): String {
        val bestAudio = audios.maxByOrNull { it.bitrate }
        // One representation per height: the highest-bitrate format at that height.
        val videoReps = videos.groupBy { it.height }
            .mapValues { (_, formats) -> formats.maxByOrNull { it.bitrate }!! }
            .toList().sortedByDescending { it.first }
        val sb = StringBuilder()
        sb.append("""<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" """)
        if (durationSecs > 0) sb.append("""mediaPresentationDuration="PT${durationSecs}S" """)
        sb.append("""minBufferTime="PT1.5S" profiles="urn:mpeg:dash:profile:isoff-live:2011"><Period>""")
        sb.append("""<AdaptationSet mimeType="video/mp4" segmentAlignment="true">""")
        for ((_, format) in videoReps) {
            val codec = format.mimeType.substringAfter("codecs=\"", "").substringBefore("\"")
            sb.append("""<Representation id="v${format.itag}" codecs="$codec" """)
            sb.append("""width="${format.width}" height="${format.height}" """)
            if (format.fps != null) sb.append("""frameRate="${format.fps}" """)
            sb.append("""bandwidth="${format.bitrate}" startWithSAP="1">""")
            sb.append("<BaseURL>${escapeXml(format.url!!)}</BaseURL>")
            appendSegmentBase(sb, format)
            sb.append("</Representation>")
        }
        sb.append("</AdaptationSet>")
        if (bestAudio != null) {
            val codec = bestAudio.mimeType.substringAfter("codecs=\"", "").substringBefore("\"")
            sb.append("""<AdaptationSet mimeType="audio/mp4" segmentAlignment="true">""")
            sb.append("""<Representation id="a${bestAudio.itag}" codecs="$codec" """)
            sb.append("""bandwidth="${bestAudio.bitrate}" """)
            if (bestAudio.audioSampleRate != null) {
                sb.append("""audioSamplingRate="${bestAudio.audioSampleRate}" """)
            }
            sb.append(">")
            sb.append("<BaseURL>${escapeXml(bestAudio.url!!)}</BaseURL>")
            appendSegmentBase(sb, bestAudio)
            sb.append("</Representation></AdaptationSet>")
        }
        sb.append("</Period></MPD>")
        return sb.toString()
    }

    private fun appendSegmentBase(
        sb: StringBuilder,
        format: PlayerResponse.StreamingData.Format,
    ) {
        val index = format.indexRange
        val init = format.initRange
        if (index != null && init != null) {
            sb.append("""<SegmentBase indexRange="${index.start}-${index.end}" timescale="1000">""")
            sb.append("""<Initialization range="${init.start}-${init.end}"/>""")
            sb.append("</SegmentBase>")
        }
    }

    private fun escapeXml(value: String): String =
        value.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

    // ---- notification ----

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Video playback",
                NotificationManager.IMPORTANCE_LOW,
            )
            (getSystemService(NOTIFICATION_SERVICE) as NotificationManager)
                .createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification {
        val meta = currentMeta
        val openIntent = Intent(this, MainActivity::class.java).let {
            PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE)
        }
        val toggleIntent = Intent(this, VideoPlaybackService::class.java)
            .setAction(ACTION_TOGGLE).let {
                PendingIntent.getService(this, 1, it, PendingIntent.FLAG_IMMUTABLE)
            }
        val stopIntent = Intent(this, VideoPlaybackService::class.java)
            .setAction(ACTION_STOP).let {
                PendingIntent.getService(this, 2, it, PendingIntent.FLAG_IMMUTABLE)
            }
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(meta?.title?.ifEmpty { "Averon Video" } ?: "Averon Video")
            .setContentText(if (player.isPlaying) "Playing" else "Paused")
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setLargeIcon(
                android.graphics.BitmapFactory.decodeResource(
                    resources,
                    com.beatwave.music.R.drawable.beatwave_logo,
                )
            )
            .setContentIntent(openIntent)
            .addAction(
                if (player.isPlaying) android.R.drawable.ic_media_pause
                else android.R.drawable.ic_media_play,
                if (player.isPlaying) "Pause" else "Play",
                toggleIntent,
            )
            .addAction(android.R.drawable.ic_delete, "Stop", stopIntent)
            .setOngoing(true)
            .build()
    }

    private fun startForegroundService() {
        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            buildNotification(),
            android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
        )
    }

    private fun updateNotification() {
        if (_nowPlaying.value != null) {
            val manager = getSystemService(NOTIFICATION_SERVICE) as NotificationManager
            manager.notify(NOTIFICATION_ID, buildNotification())
        }
    }

    companion object {
        const val ACTION_PLAY = "com.beatwave.music.video.PLAY"
        const val ACTION_TOGGLE = "com.beatwave.music.video.TOGGLE"
        const val ACTION_STOP = "com.beatwave.music.video.STOP"
        const val EXTRA_VIDEO_ID = "video_id"
        const val EXTRA_TITLE = "title"
        const val EXTRA_THUMB = "thumb"

        private const val CHANNEL_ID = "averon_video_playback"
        private const val NOTIFICATION_ID = 2001

        private val _nowPlaying = MutableStateFlow<VideoNowPlaying?>(null)
        val nowPlaying: StateFlow<VideoNowPlaying?> = _nowPlaying.asStateFlow()

        private val _selectedHeight = MutableStateFlow<Int?>(null)
        val selectedHeight: StateFlow<Int?> = _selectedHeight.asStateFlow()

        private val _error = MutableStateFlow<String?>(null)
        val error: StateFlow<String?> = _error.asStateFlow()

        /** Set by VideoScreen just before navigating to the player screen. */
        var pendingVideo: VideoMeta? = null

        fun startVideo(context: Context, meta: VideoMeta) {
            pendingVideo = meta
            val intent = Intent(context, VideoPlaybackService::class.java).apply {
                action = ACTION_PLAY
                putExtra(EXTRA_VIDEO_ID, meta.videoId)
                putExtra(EXTRA_TITLE, meta.title)
                putExtra(EXTRA_THUMB, meta.thumbnailUrl)
            }
            androidx.core.content.ContextCompat.startForegroundService(context, intent)
        }
    }
}
