/**
 * BeatWave Project (C) 2026
 * Licensed under GPL-3.0 | See git history for contributors
 *
 * Averon Video tab: a YouTube-style home feed — logo header, category chips
 * backed by YouTube's real kiosk feeds (Trending, Music, Gaming, Live,
 * Podcasts, Movies), long-video cards, and a Shorts shelf. Search covers any
 * public YouTube video through the NewPipe extractor; playback is ad-free
 * by design (direct streams, no ad slots).
 */

package com.beatwave.music.ui.screens.video

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import coil3.compose.AsyncImage
import com.beatwave.music.LocalPlayerAwareWindowInsets
import com.beatwave.music.R
import com.beatwave.music.playback.VideoMeta
import com.beatwave.music.playback.VideoPlaybackService
import com.music.innertube.pages.VideoSearch
import com.music.innertube.pages.VideoSearchItem
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

fun formatVideoDuration(totalSecs: Long): String {
    val hours = totalSecs / 3600
    val minutes = (totalSecs % 3600) / 60
    val seconds = totalSecs % 60
    return if (hours > 0) {
        "%d:%02d:%02d".format(hours, minutes, seconds)
    } else {
        "%d:%02d".format(minutes, seconds)
    }
}

fun formatViewCount(count: Long): String = when {
    count >= 1_000_000_000 -> "%.1fB views".format(count / 1_000_000_000.0)
    count >= 1_000_000 -> "%.1fM views".format(count / 1_000_000.0)
    count >= 1_000 -> "%.1fK views".format(count / 1_000.0)
    count > 0 -> "$count views"
    else -> ""
}

/** A chip on the YouTube-style category row and the feed behind it. */
private enum class VideoFeed(val label: String, val kioskId: String?) {
    ALL("All", VideoSearch.KIOSK_TRENDING),
    NEW_TO_YOU("New to you", VideoSearch.KIOSK_TRENDING),
    MUSIC("Music", VideoSearch.KIOSK_MUSIC),
    GAMING("Gaming", VideoSearch.KIOSK_GAMING),
    LIVE("Live", VideoSearch.KIOSK_LIVE),
    PODCASTS("Podcasts", VideoSearch.KIOSK_PODCASTS),
    MOVIES("Movies", VideoSearch.KIOSK_MOVIES),
    // No News kiosk exists — fall back to a plain search.
    NEWS("News", null),
}

/** Shorts are short-form: up to 3 minutes. */
private const val SHORTS_MAX_SECS = 180L

fun openVideo(navController: NavController, item: VideoSearchItem) {
    VideoPlaybackService.pendingVideo = VideoMeta(
        item.videoId,
        item.title,
        item.thumbnailUrl,
    )
    navController.navigate("video_player/${item.videoId}")
}

@Composable
fun VideoCard(
    item: VideoSearchItem,
    onClick: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(16f / 9f)
                .clip(RoundedCornerShape(12.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant),
        ) {
            if (item.thumbnailUrl.isNotEmpty()) {
                AsyncImage(
                    model = item.thumbnailUrl,
                    contentDescription = null,
                    modifier = Modifier.fillMaxSize(),
                    contentScale = ContentScale.Crop,
                )
            }
            // Duration badge (YouTube-style, bottom-right) or a LIVE badge.
            val badge = if (item.isLive) "LIVE" else
                if (item.durationSecs > 0) formatVideoDuration(item.durationSecs) else ""
            if (badge.isNotEmpty()) {
                Box(
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .padding(6.dp)
                        .clip(RoundedCornerShape(4.dp))
                        .background(if (item.isLive) Color(0xFFCC0000) else Color.Black.copy(alpha = 0.8f))
                        .padding(horizontal = 5.dp, vertical = 2.dp),
                ) {
                    Text(
                        text = badge,
                        style = MaterialTheme.typography.labelSmall,
                        color = Color.White,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
        Spacer(modifier = Modifier.height(8.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.Top,
        ) {
            if (item.channelAvatarUrl.isNotEmpty()) {
                AsyncImage(
                    model = item.channelAvatarUrl,
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
                    text = item.title,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(modifier = Modifier.height(2.dp))
                val meta = listOf(
                    item.channel,
                    formatViewCount(item.viewCount),
                    item.uploadDate,
                ).filter { it.isNotEmpty() }.joinToString(" • ")
                if (meta.isNotEmpty()) {
                    Text(
                        text = meta,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

@Composable
private fun ShortsShelf(
    items: List<VideoSearchItem>,
    navController: NavController,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painter = painterResource(R.drawable.slow_motion_video),
                contentDescription = null,
                tint = Color(0xFFFF0000),
                modifier = Modifier.size(22.dp),
            )
            Spacer(modifier = Modifier.width(6.dp))
            Text(
                text = "Shorts",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
            )
        }
        LazyRow(
            contentPadding = PaddingValues(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(items, key = { it.videoId }) { item ->
                Column(
                    modifier = Modifier
                        .width(148.dp)
                        .clickable { openVideo(navController, item) },
                ) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .aspectRatio(9f / 16f)
                            .clip(RoundedCornerShape(12.dp))
                            .background(MaterialTheme.colorScheme.surfaceVariant),
                    ) {
                        val thumb = item.portraitThumbnailUrl.ifEmpty { item.thumbnailUrl }
                        if (thumb.isNotEmpty()) {
                            AsyncImage(
                                model = thumb,
                                contentDescription = null,
                                modifier = Modifier.fillMaxSize(),
                                contentScale = ContentScale.Crop,
                            )
                        }
                    }
                    Spacer(modifier = Modifier.height(6.dp))
                    Text(
                        text = item.title,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Medium,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    val views = formatViewCount(item.viewCount)
                    if (views.isNotEmpty()) {
                        Text(
                            text = views,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun VideoScreen(navController: NavController) {
    var searchOpen by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    var searchResults by remember { mutableStateOf<List<VideoSearchItem>>(emptyList()) }
    var searchLoading by remember { mutableStateOf(false) }
    var searchError by remember { mutableStateOf<String?>(null) }
    var hasSearched by remember { mutableStateOf(false) }

    var selectedFeed by remember { mutableStateOf(VideoFeed.ALL) }
    var feedLong by remember { mutableStateOf<List<VideoSearchItem>>(emptyList()) }
    var feedShorts by remember { mutableStateOf<List<VideoSearchItem>>(emptyList()) }
    var feedLoading by remember { mutableStateOf(false) }
    var feedError by remember { mutableStateOf<String?>(null) }

    val scope = rememberCoroutineScope()
    val nowPlaying by VideoPlaybackService.nowPlaying.collectAsState()

    fun loadFeed(feed: VideoFeed) {
        feedLoading = true
        feedError = null
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                if (feed.kioskId != null) {
                    VideoSearch.kioskVideos(feed.kioskId)
                } else {
                    VideoSearch.searchVideos("news")
                }
            }
            result.onSuccess { items ->
                val shorts = items.filter { it.durationSecs in 1..SHORTS_MAX_SECS && !it.isLive }
                val long = items.filter { it !in shorts }
                var allShorts = shorts
                // The All feed's Shorts shelf gets a top-up from the Music and
                // Gaming feeds when Trending alone has too few short videos.
                if (feed == VideoFeed.ALL && allShorts.size < 4) {
                    val extra = withContext(Dispatchers.IO) {
                        listOf(VideoSearch.KIOSK_MUSIC, VideoSearch.KIOSK_GAMING)
                            .flatMap { VideoSearch.kioskVideos(it).getOrNull().orEmpty() }
                    }.filter { it.durationSecs in 1..SHORTS_MAX_SECS && !it.isLive }
                    val seen = allShorts.map { it.videoId }.toSet()
                    allShorts = allShorts + extra.filter { it.videoId !in seen }
                }
                feedLong = long
                feedShorts = allShorts.take(20)
            }.onFailure {
                feedError = "Couldn't load videos. Check your connection and try again."
            }
            feedLoading = false
        }
    }

    fun doSearch() {
        val q = query.trim()
        if (q.isEmpty()) return
        searchLoading = true
        searchError = null
        hasSearched = true
        scope.launch {
            val found = withContext(Dispatchers.IO) {
                VideoSearch.searchVideos(q)
            }
            found.onSuccess { searchResults = it }
                .onFailure { searchError = "Search failed. Check your connection and try again." }
            searchLoading = false
        }
    }

    LaunchedEffect(selectedFeed) { loadFeed(selectedFeed) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(LocalPlayerAwareWindowInsets.current.asPaddingValues()),
    ) {
        // YouTube-style header: Averon logo + wordmark, search on the right.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Image(
                painter = painterResource(R.drawable.beatwave_logo),
                contentDescription = "Averon Logo",
                modifier = Modifier
                    .size(32.dp)
                    .clip(RoundedCornerShape(8.dp)),
                contentScale = ContentScale.Crop,
            )
            Spacer(modifier = Modifier.width(8.dp))
            Text(
                text = "Averon",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.weight(1f),
            )
            IconButton(onClick = {
                searchOpen = !searchOpen
                if (!searchOpen) {
                    hasSearched = false
                    searchResults = emptyList()
                }
            }) {
                Icon(
                    painter = painterResource(R.drawable.search),
                    contentDescription = stringResource(R.string.search),
                )
            }
        }

        if (searchOpen) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    modifier = Modifier.weight(1f),
                    placeholder = { Text(stringResource(R.string.video_search_hint)) },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { doSearch() }),
                )
                Spacer(modifier = Modifier.width(8.dp))
                IconButton(onClick = { doSearch() }) {
                    Icon(
                        painter = painterResource(R.drawable.search),
                        contentDescription = stringResource(R.string.search),
                    )
                }
            }
        }

        // Category chips (pinned, like YouTube's).
        LazyRow(
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(VideoFeed.entries, key = { it.name }) { feed ->
                FilterChip(
                    selected = selectedFeed == feed && !hasSearched,
                    onClick = {
                        selectedFeed = feed
                        hasSearched = false
                        searchResults = emptyList()
                    },
                    label = { Text(feed.label) },
                )
            }
        }

        nowPlaying?.let { playing ->
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 4.dp)
                    .clickable {
                        navController.navigate("video_player/${playing.meta.videoId}")
                    },
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (playing.meta.thumbnailUrl.isNotEmpty()) {
                        AsyncImage(
                            model = playing.meta.thumbnailUrl,
                            contentDescription = null,
                            modifier = Modifier
                                .size(width = 96.dp, height = 54.dp),
                            contentScale = ContentScale.Crop,
                        )
                        Spacer(modifier = Modifier.width(12.dp))
                    }
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = stringResource(R.string.video_now_playing),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.primary,
                        )
                        Text(
                            text = playing.meta.title.ifEmpty { "Video" },
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    Button(onClick = {
                        navController.navigate("video_player/${playing.meta.videoId}")
                    }) {
                        Text(stringResource(R.string.video_open_player))
                    }
                }
            }
        }

        val showingSearch = searchOpen && hasSearched
        when {
            (showingSearch && searchLoading) || (!showingSearch && feedLoading && feedLong.isEmpty() && feedShorts.isEmpty()) -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    CircularProgressIndicator()
                }
            }
            showingSearch && searchError != null -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(searchError ?: "")
                        Spacer(modifier = Modifier.height(8.dp))
                        Button(onClick = { doSearch() }) {
                            Text(stringResource(R.string.video_retry))
                        }
                    }
                }
            }
            !showingSearch && feedError != null && feedLong.isEmpty() && feedShorts.isEmpty() -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(feedError ?: "")
                        Spacer(modifier = Modifier.height(8.dp))
                        Button(onClick = { loadFeed(selectedFeed) }) {
                            Text(stringResource(R.string.video_retry))
                        }
                    }
                }
            }
            showingSearch && searchResults.isEmpty() -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(stringResource(R.string.video_no_results))
                }
            }
            showingSearch -> {
                LazyColumn(modifier = Modifier.fillMaxSize()) {
                    items(searchResults, key = { it.videoId }) { item ->
                        VideoCard(item = item, onClick = { openVideo(navController, item) })
                    }
                }
            }
            else -> {
                LazyColumn(modifier = Modifier.fillMaxSize()) {
                    val head = feedLong.take(4)
                    items(head, key = { it.videoId }) { item ->
                        VideoCard(item = item, onClick = { openVideo(navController, item) })
                    }
                    if (feedShorts.isNotEmpty()) {
                        item(key = "shorts_shelf") {
                            ShortsShelf(items = feedShorts, navController = navController)
                        }
                    }
                    items(feedLong.drop(4), key = { it.videoId }) { item ->
                        VideoCard(item = item, onClick = { openVideo(navController, item) })
                    }
                }
            }
        }
    }
}
