/**
 * BeatWave Project (C) 2026
 * Licensed under GPL-3.0 | See git history for contributors
 *
 * In-app video player: the video surface is bound to the player owned by
 * [VideoPlaybackService], so leaving this screen (or turning the screen off)
 * keeps the audio playing in the background. Quality selection caps the video
 * height; Auto lets ExoPlayer pick the highest available quality.
 *
 * Averon: below the player this is a YouTube-style watch page — title,
 * views/time, expandable description, channel row with a real Subscribe
 * button, Like/Dislike/Share pills, real comments, and Up next.
 */

package com.beatwave.music.ui.screens.video

import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.ActivityInfo
import android.os.IBinder
import android.view.ViewGroup
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import androidx.navigation.NavController
import coil3.compose.AsyncImage
import com.beatwave.music.R
import com.beatwave.music.constants.InnerTubeCookieKey
import com.beatwave.music.playback.VideoMeta
import com.beatwave.music.playback.VideoPlaybackService
import com.beatwave.music.utils.rememberPreference
import com.music.innertube.YouTube
import com.music.innertube.pages.VideoComment
import com.music.innertube.pages.VideoDetails
import com.music.innertube.pages.VideoDetailsData
import com.music.innertube.utils.parseCookieString
import java.time.Duration
import java.time.Instant
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Short count without the "views" suffix, e.g. 1.2M / 45K / 123. */
private fun formatShortCount(count: Long): String = when {
    count >= 1_000_000_000 -> "%.1fB".format(count / 1_000_000_000.0)
    count >= 1_000_000 -> "%.1fM".format(count / 1_000_000.0)
    count >= 1_000 -> "%.1fK".format(count / 1_000.0)
    count > 0 -> "$count"
    else -> ""
}

private fun formatSubs(count: Long): String = when {
    count <= 0 -> ""
    count >= 1_000_000 -> "%.1fM subscribers".format(count / 1_000_000.0)
    count >= 1_000 -> "%.1fK subscribers".format(count / 1_000.0)
    else -> "$count subscribers"
}

private fun formatRelativeTime(instant: Instant?): String {
    if (instant == null) return ""
    val dur = Duration.between(instant, Instant.now())
    if (dur.isNegative) return ""
    fun ago(value: Long, unit: String) =
        "$value $unit${if (value == 1L) "" else "s"} ago"
    return when {
        dur.toMinutes() < 1 -> "just now"
        dur.toMinutes() < 60 -> ago(dur.toMinutes(), "minute")
        dur.toHours() < 24 -> ago(dur.toHours(), "hour")
        dur.toDays() < 7 -> ago(dur.toDays(), "day")
        dur.toDays() < 30 -> ago(dur.toDays() / 7, "week")
        dur.toDays() < 365 -> ago(dur.toDays() / 30, "month")
        else -> ago(dur.toDays() / 365, "year")
    }
}

@Composable
private fun OverlayButton(icon: Int, onClick: () -> Unit) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(36.dp)
            .background(Color.Black.copy(alpha = 0.5f), CircleShape),
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = Color.White,
            modifier = Modifier.size(20.dp),
        )
    }
}

@Composable
private fun PillButton(
    icon: Int,
    text: String,
    selected: Boolean,
    onClick: () -> Unit,
    enabled: Boolean = true,
    rotateIcon: Boolean = false,
) {
    Surface(
        shape = RoundedCornerShape(20.dp),
        color = if (selected) MaterialTheme.colorScheme.primaryContainer
        else MaterialTheme.colorScheme.surfaceVariant,
        modifier = Modifier.clickable(enabled = enabled, onClick = onClick),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painter = painterResource(icon),
                contentDescription = null,
                modifier = Modifier
                    .size(18.dp)
                    .then(if (rotateIcon) Modifier.rotate(180f) else Modifier),
            )
            if (text.isNotEmpty()) {
                Spacer(modifier = Modifier.width(6.dp))
                Text(
                    text = text,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                )
            }
        }
    }
}

@Composable
fun VideoPlayerScreen(
    navController: NavController,
    videoId: String,
) {
    val context = LocalContext.current
    var service by remember { mutableStateOf<VideoPlaybackService?>(null) }
    var player by remember { mutableStateOf<ExoPlayer?>(null) }
    var isFullscreen by remember { mutableStateOf(false) }

    val nowPlaying by VideoPlaybackService.nowPlaying.collectAsState()
    val selectedHeight by VideoPlaybackService.selectedHeight.collectAsState()
    val error by VideoPlaybackService.error.collectAsState()

    // Watch-page state. Details failures never touch playback: the player
    // is driven by the service, details are a separate best-effort load.
    val innerTubeCookie by rememberPreference(InnerTubeCookieKey, "")
    val isLoggedIn = remember(innerTubeCookie) {
        "SAPISID" in parseCookieString(innerTubeCookie)
    }
    var details by remember { mutableStateOf<VideoDetailsData?>(null) }
    var comments by remember { mutableStateOf<List<VideoComment>>(emptyList()) }
    var likeCount by remember { mutableLongStateOf(-1L) }
    var dislikeCount by remember { mutableLongStateOf(-1L) }
    var liked by remember { mutableStateOf(false) }
    var disliked by remember { mutableStateOf(false) }
    var subscribed by remember { mutableStateOf(false) }
    var actionBusy by remember { mutableStateOf(false) }
    var descExpanded by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    val connection = remember {
        object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
                val svc = (binder as VideoPlaybackService.VideoBinder).getService()
                service = svc
                player = binder.getPlayer()
            }

            override fun onServiceDisconnected(name: ComponentName?) {
                service = null
                player = null
            }
        }
    }

    LaunchedEffect(videoId) {
        val playingId = VideoPlaybackService.nowPlaying.value?.meta?.videoId
        if (playingId != videoId) {
            val meta = VideoPlaybackService.pendingVideo
                ?.takeIf { it.videoId == videoId }
                ?: VideoMeta(videoId, "", "")
            VideoPlaybackService.startVideo(context, meta)
        }
    }

    LaunchedEffect(videoId) {
        details = null
        comments = emptyList()
        likeCount = -1L
        dislikeCount = -1L
        liked = false
        disliked = false
        subscribed = false
        descExpanded = false
        launch { VideoDetails.videoDetails(videoId).onSuccess { details = it } }
        launch { VideoDetails.videoComments(videoId).onSuccess { comments = it } }
        launch {
            YouTube.videoLikeCounts(videoId).onSuccess { (likes, dislikes) ->
                likeCount = likes
                dislikeCount = dislikes
            }
        }
    }

    DisposableEffect(Unit) {
        context.bindService(
            Intent(context, VideoPlaybackService::class.java),
            connection,
            Context.BIND_AUTO_CREATE,
        )
        onDispose {
            try {
                context.unbindService(connection)
            } catch (_: Exception) {
            }
        }
    }

    DisposableEffect(isFullscreen) {
        val activity = context as? Activity
        activity?.requestedOrientation = if (isFullscreen) {
            ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        } else {
            ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
        onDispose {
            activity?.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
    }

    // Up-next replaces this page (like YouTube) instead of stacking players.
    fun playNext(item: com.music.innertube.pages.VideoSearchItem) {
        VideoPlaybackService.pendingVideo = VideoMeta(
            item.videoId,
            item.title,
            item.thumbnailUrl,
        )
        navController.navigate("video_player/${item.videoId}") {
            popUpTo("video_player/$videoId") { inclusive = true }
        }
    }

    Column(modifier = Modifier.fillMaxSize()) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .then(if (isFullscreen) Modifier.weight(1f) else Modifier.aspectRatio(16f / 9f)),
        ) {
            val currentPlayer = player
            if (currentPlayer != null) {
                AndroidView(
                    factory = { ctx ->
                        PlayerView(ctx).apply {
                            this.player = currentPlayer
                            useController = false
                            layoutParams = ViewGroup.LayoutParams(
                                ViewGroup.LayoutParams.MATCH_PARENT,
                                ViewGroup.LayoutParams.MATCH_PARENT,
                            )
                        }
                    },
                    update = { view ->
                        view.player = currentPlayer
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            }
            if (currentPlayer == null || error == null &&
                (currentPlayer?.playbackState == Player.STATE_BUFFERING ||
                    currentPlayer?.playbackState == Player.STATE_IDLE)
            ) {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    CircularProgressIndicator()
                }
            }
            if (error != null) {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(error ?: "")
                        Spacer(modifier = Modifier.height(8.dp))
                        Button(onClick = {
                            val meta = VideoPlaybackService.pendingVideo
                                ?.takeIf { it.videoId == videoId }
                                ?: VideoMeta(videoId, "", "")
                            VideoPlaybackService.startVideo(context, meta)
                        }) {
                            Text(stringResource(R.string.video_retry))
                        }
                    }
                }
            }
            // YouTube-style overlay: back + fullscreen over the video.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(8.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                OverlayButton(R.drawable.chevron_leftpx) {
                    navController.popBackStack()
                }
                OverlayButton(
                    if (isFullscreen) R.drawable.fullscreen_exit else R.drawable.fullscreen,
                ) {
                    isFullscreen = !isFullscreen
                }
            }
        }

        VideoControls(
            player = player,
            service = service,
            heights = nowPlaying?.heights ?: emptyList(),
            selectedHeight = selectedHeight,
            isDash = nowPlaying?.isDash ?: false,
        )

        if (!isFullscreen) {
            val watchTitle = nowPlaying?.meta?.title?.takeIf { it.isNotEmpty() }
                ?: details?.title?.takeIf { it.isNotEmpty() }
                ?: "Video"
            val metaLine = listOf(
                details?.let { formatViewCount(it.viewCount) } ?: "",
                details?.let { formatRelativeTime(it.uploadInstant) } ?: "",
            ).filter { it.isNotEmpty() }.joinToString(" • ")

            LazyColumn(modifier = Modifier.weight(1f)) {
                // Title + views/time + expandable description.
                item(key = "watch_header") {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text(
                            text = watchTitle,
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                            maxLines = if (descExpanded) Int.MAX_VALUE else 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Spacer(modifier = Modifier.height(4.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            if (metaLine.isNotEmpty()) {
                                Text(
                                    text = metaLine,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    modifier = Modifier.weight(1f, fill = false),
                                )
                                Spacer(modifier = Modifier.width(8.dp))
                            }
                            Text(
                                text = if (descExpanded) "Show less" else "...more",
                                style = MaterialTheme.typography.bodySmall,
                                fontWeight = FontWeight.Bold,
                                modifier = Modifier.clickable {
                                    descExpanded = !descExpanded
                                },
                            )
                        }
                        if (descExpanded && !details?.description.isNullOrEmpty()) {
                            Spacer(modifier = Modifier.height(8.dp))
                            Text(
                                text = details?.description ?: "",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                }

                // Channel row + Subscribe.
                details?.let { d ->
                    item(key = "watch_channel") {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 12.dp, vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            if (d.uploaderAvatarUrl.isNotEmpty()) {
                                AsyncImage(
                                    model = d.uploaderAvatarUrl,
                                    contentDescription = null,
                                    modifier = Modifier
                                        .size(36.dp)
                                        .clip(CircleShape),
                                    contentScale = ContentScale.Crop,
                                )
                            } else {
                                Image(
                                    painter = painterResource(R.drawable.beatwave_logo),
                                    contentDescription = null,
                                    modifier = Modifier
                                        .size(36.dp)
                                        .clip(CircleShape),
                                    contentScale = ContentScale.Crop,
                                )
                            }
                            Spacer(modifier = Modifier.width(10.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text(
                                    text = d.uploaderName,
                                    style = MaterialTheme.typography.bodyMedium,
                                    fontWeight = FontWeight.Bold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                val subs = formatSubs(d.uploaderSubs)
                                if (subs.isNotEmpty()) {
                                    Text(
                                        text = subs,
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                            Button(
                                onClick = {
                                    if (!isLoggedIn) {
                                        navController.navigate("login")
                                    } else if (!actionBusy) {
                                        actionBusy = true
                                        scope.launch {
                                            val target = !subscribed
                                            val channelId = VideoDetails
                                                .resolveChannelId(d.uploaderUrl)
                                                .getOrNull()
                                            val ok = if (channelId != null) {
                                                YouTube.subscribeChannel(channelId, target)
                                                    .isSuccess
                                            } else false
                                            if (ok) subscribed = target
                                            actionBusy = false
                                        }
                                    }
                                },
                                enabled = !actionBusy,
                            ) {
                                Text(if (subscribed) "Subscribed" else "Subscribe")
                            }
                        }
                    }
                }

                // Like / Dislike / Share pills.
                item(key = "watch_actions") {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 12.dp, vertical = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        PillButton(
                            icon = R.drawable.thumb_up_like,
                            text = if (likeCount >= 0) formatShortCount(likeCount) else "Like",
                            selected = liked,
                            enabled = !actionBusy,
                            onClick = {
                                if (!isLoggedIn) {
                                    navController.navigate("login")
                                } else if (!actionBusy) {
                                    actionBusy = true
                                    scope.launch {
                                        val target = !liked
                                        if (YouTube.likeVideo(videoId, target).isSuccess) {
                                            liked = target
                                            if (target) disliked = false
                                        }
                                        actionBusy = false
                                    }
                                }
                            },
                        )
                        PillButton(
                            icon = R.drawable.thumb_up_like,
                            text = if (dislikeCount >= 0) formatShortCount(dislikeCount) else "Dislike",
                            selected = disliked,
                            enabled = !actionBusy,
                            rotateIcon = true,
                            onClick = {
                                if (!isLoggedIn) {
                                    navController.navigate("login")
                                } else if (!actionBusy) {
                                    actionBusy = true
                                    scope.launch {
                                        val target = !disliked
                                        val ok = if (target) {
                                            YouTube.dislikeVideo(videoId).isSuccess
                                        } else {
                                            YouTube.likeVideo(videoId, false).isSuccess
                                        }
                                        if (ok) {
                                            disliked = target
                                            if (target) liked = false
                                        }
                                        actionBusy = false
                                    }
                                }
                            },
                        )
                        PillButton(
                            icon = R.drawable.share,
                            text = "Share",
                            selected = false,
                            onClick = {
                                val shareIntent = Intent(Intent.ACTION_SEND).apply {
                                    type = "text/plain"
                                    putExtra(
                                        Intent.EXTRA_TEXT,
                                        "https://www.youtube.com/watch?v=$videoId",
                                    )
                                }
                                context.startActivity(
                                    Intent.createChooser(shareIntent, "Share video"),
                                )
                            },
                        )
                    }
                }

                // Comments.
                item(key = "comments_header") {
                    Text(
                        text = "Comments",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                    )
                }
                if (comments.isEmpty()) {
                    item(key = "comments_empty") {
                        Text(
                            text = "No comments yet",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                        )
                    }
                } else {
                    itemsIndexed(comments, key = { index, _ -> index }) { _, comment ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 12.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.Top,
                        ) {
                            if (comment.avatarUrl.isNotEmpty()) {
                                AsyncImage(
                                    model = comment.avatarUrl,
                                    contentDescription = null,
                                    modifier = Modifier
                                        .size(32.dp)
                                        .clip(CircleShape),
                                    contentScale = ContentScale.Crop,
                                )
                            } else {
                                Image(
                                    painter = painterResource(R.drawable.beatwave_logo),
                                    contentDescription = null,
                                    modifier = Modifier
                                        .size(32.dp)
                                        .clip(CircleShape),
                                    contentScale = ContentScale.Crop,
                                )
                            }
                            Spacer(modifier = Modifier.width(10.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text(
                                    text = listOf(comment.author, comment.timeAgo)
                                        .filter { it.isNotEmpty() }
                                        .joinToString(" • "),
                                    style = MaterialTheme.typography.bodySmall,
                                    fontWeight = FontWeight.Bold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Spacer(modifier = Modifier.height(2.dp))
                                Text(
                                    text = comment.text,
                                    style = MaterialTheme.typography.bodyMedium,
                                )
                                if (comment.likeCount > 0) {
                                    Spacer(modifier = Modifier.height(4.dp))
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Icon(
                                            painter = painterResource(R.drawable.thumb_up_like),
                                            contentDescription = null,
                                            modifier = Modifier.size(14.dp),
                                        )
                                        Spacer(modifier = Modifier.width(4.dp))
                                        Text(
                                            text = formatShortCount(comment.likeCount.toLong()),
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    }
                                }
                            }
                        }
                    }
                }

                // Up next.
                val related = details?.related.orEmpty()
                if (related.isNotEmpty()) {
                    item(key = "up_next_header") {
                        Text(
                            text = "Up next",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                        )
                    }
                    items(related, key = { it.videoId }) { item ->
                        VideoCard(item = item, onClick = { playNext(item) })
                    }
                }
            }
        }
    }
}

@Composable
private fun VideoControls(
    player: ExoPlayer?,
    service: VideoPlaybackService?,
    heights: List<Int>,
    selectedHeight: Int?,
    isDash: Boolean,
) {
    var position by remember { mutableLongStateOf(0L) }
    var duration by remember { mutableLongStateOf(0L) }
    var isPlaying by remember { mutableStateOf(false) }
    var qualityMenuOpen by remember { mutableStateOf(false) }

    LaunchedEffect(player) {
        while (true) {
            val p = player
            if (p != null) {
                position = p.currentPosition
                duration = p.duration.coerceAtLeast(0L)
                isPlaying = p.isPlaying
            }
            delay(500)
        }
    }

    Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
        Slider(
            value = if (duration > 0) position.toFloat() / duration.toFloat() else 0f,
            onValueChange = { fraction ->
                if (duration > 0) {
                    player?.seekTo((fraction * duration).toLong())
                }
            },
            modifier = Modifier.fillMaxWidth(),
            enabled = player != null && duration > 0,
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = "${formatVideoDuration(position / 1000)} / ${formatVideoDuration(duration / 1000)}",
                style = MaterialTheme.typography.bodySmall,
            )
            Spacer(modifier = Modifier.weight(1f))
            if (isDash && heights.isNotEmpty()) {
                Box {
                    Button(onClick = { qualityMenuOpen = true }) {
                        Text(
                            selectedHeight?.let { "${it}p" }
                                ?: stringResource(R.string.video_quality_auto),
                        )
                    }
                    DropdownMenu(
                        expanded = qualityMenuOpen,
                        onDismissRequest = { qualityMenuOpen = false },
                    ) {
                        DropdownMenuItem(
                            text = { Text(stringResource(R.string.video_quality_auto)) },
                            onClick = {
                                service?.setQuality(null)
                                qualityMenuOpen = false
                            },
                        )
                        heights.forEach { height ->
                            DropdownMenuItem(
                                text = { Text("${height}p") },
                                onClick = {
                                    service?.setQuality(height)
                                    qualityMenuOpen = false
                                },
                            )
                        }
                    }
                }
                Spacer(modifier = Modifier.width(8.dp))
            }
            IconButton(
                onClick = { service?.toggle() },
                enabled = player != null,
            ) {
                Icon(
                    painter = painterResource(
                        if (isPlaying) R.drawable.pause else R.drawable.play,
                    ),
                    contentDescription = null,
                )
            }
        }
        Text(
            text = stringResource(R.string.video_background_note),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
