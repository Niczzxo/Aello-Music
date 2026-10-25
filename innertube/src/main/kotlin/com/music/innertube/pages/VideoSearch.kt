/**
 * BeatWave Project (C) 2026
 * Licensed under GPL-3.0 | See git history for contributors
 *
 * General YouTube video search (not the music-only search in YouTube.search).
 * Uses the NewPipe extractor already bundled in this module: it searches
 * youtube.com directly, so results cover any public video, not just music.
 *
 * Averon: also exposes YouTube's kiosk feeds (Trending, Music, Gaming, Live,
 * Podcasts, Movies) — these are the same feeds behind YouTube's home chips,
 * so the Video tab can look like a real YouTube home feed.
 */

package com.music.innertube.pages

import com.music.innertube.NewPipeExtractor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.schabi.newpipe.extractor.NewPipe
import org.schabi.newpipe.extractor.stream.StreamInfoItem
import org.schabi.newpipe.extractor.stream.StreamType

data class VideoSearchItem(
    val videoId: String,
    val title: String,
    val channel: String,
    val durationSecs: Long,
    val viewCount: Long,
    val thumbnailUrl: String,
    val channelAvatarUrl: String = "",
    val uploadDate: String = "",
    val isLive: Boolean = false,
) {
    /** Tallest thumbnail when one exists (Shorts covers), else the main thumb. */
    val portraitThumbnailUrl: String
        get() = portraitThumb.ifEmpty { thumbnailUrl }

    // Set by the mapper; kept out of the constructor so callers stay simple.
    var portraitThumb: String = ""
        internal set
}

object VideoSearch {

    /** Kiosk ids registered by NewPipe's YoutubeService (verified from its bytecode). */
    const val KIOSK_TRENDING = "Trending"
    const val KIOSK_MUSIC = "trending_music"
    const val KIOSK_GAMING = "trending_gaming"
    const val KIOSK_LIVE = "live"
    const val KIOSK_PODCASTS = "trending_podcasts_episodes"
    const val KIOSK_MOVIES = "trending_movies_and_shows"

    suspend fun searchVideos(query: String): Result<List<VideoSearchItem>> =
        withContext(Dispatchers.IO) {
            runCatching {
                NewPipeExtractor.init()
                val service = NewPipe.getService(0)
                val handler = try {
                    service.searchQHFactory.fromQuery(query, listOf("videos"), "")
                } catch (_: Exception) {
                    service.searchQHFactory.fromQuery(query)
                }
                val extractor = service.getSearchExtractor(handler)
                extractor.fetchPage()
                extractor.initialPage.items
                    .filterIsInstance<StreamInfoItem>()
                    .mapNotNull(::mapItem)
            }
        }

    /** One kiosk feed (Trending, Music, Gaming, Live, Podcasts, Movies). */
    suspend fun kioskVideos(kioskId: String): Result<List<VideoSearchItem>> =
        withContext(Dispatchers.IO) {
            runCatching {
                NewPipeExtractor.init()
                val service = NewPipe.getService(0)
                val kioskList = service.kioskList
                val extractor = try {
                    kioskList.getExtractorById(kioskId, null)
                } catch (_: Exception) {
                    // Unknown id (extractor version drift) -> default Trending feed.
                    kioskList.defaultKioskExtractor
                }
                extractor.fetchPage()
                extractor.initialPage.items
                    .filterIsInstance<StreamInfoItem>()
                    .mapNotNull(::mapItem)
            }
        }

    private fun mapItem(item: StreamInfoItem): VideoSearchItem? {
        val url = item.url ?: return null
        val videoId = extractVideoId(url) ?: return null
        val thumbs = item.thumbnails
        val thumb = thumbs.maxByOrNull { it.height * it.width }?.url
            ?: thumbs.firstOrNull()?.url
            ?: ""
        val portrait = thumbs.filter { it.height > it.width }
            .maxByOrNull { it.height * it.width }?.url
            ?: ""
        val avatars = item.uploaderAvatars
        val avatar = avatars.maxByOrNull { it.height * it.width }?.url
            ?: avatars.firstOrNull()?.url
            ?: ""
        return VideoSearchItem(
            videoId = videoId,
            title = item.name,
            channel = item.uploaderName ?: "",
            durationSecs = item.duration.takeIf { it >= 0 } ?: 0L,
            viewCount = item.viewCount.takeIf { it >= 0 } ?: 0L,
            thumbnailUrl = thumb,
            channelAvatarUrl = avatar,
            uploadDate = item.textualUploadDate ?: "",
            isLive = item.streamType == StreamType.LIVE_STREAM,
        ).apply { portraitThumb = portrait }
    }

    private fun extractVideoId(url: String): String? {
        val id = when {
            "/shorts/" in url -> url.substringAfter("/shorts/").substringBefore("?").substringBefore("&").substringBefore("/")
            "youtu.be/" in url -> url.substringAfter("youtu.be/").substringBefore("?").substringBefore("&").substringBefore("/")
            "v=" in url -> url.substringAfter("v=").substringBefore("&")
            "/embed/" in url -> url.substringAfter("/embed/").substringBefore("?").substringBefore("&").substringBefore("/")
            else -> ""
        }
        return id.ifEmpty { null }
    }
}

/** Full watch-page data for one video (NewPipe stream info + related). */
data class VideoDetailsData(
    val title: String,
    val viewCount: Long,
    val uploadInstant: java.time.Instant?,
    val description: String,
    val uploaderName: String,
    val uploaderAvatarUrl: String,
    val uploaderSubs: Long,
    val uploaderUrl: String,
    val related: List<VideoSearchItem>,
)

data class VideoComment(
    val author: String,
    val avatarUrl: String,
    val text: String,
    val likeCount: Int,
    val timeAgo: String,
)

/** Watch-page helpers: details, comments, channel-id resolution. */
object VideoDetails {

    suspend fun videoDetails(videoId: String): Result<VideoDetailsData> =
        withContext(Dispatchers.IO) {
            runCatching {
                NewPipeExtractor.init()
                val service = NewPipe.getService(0)
                val info = org.schabi.newpipe.extractor.stream.StreamInfo.getInfo(
                    service,
                    "https://www.youtube.com/watch?v=$videoId",
                )
                val avatars = info.uploaderAvatars
                VideoDetailsData(
                    title = info.name ?: "",
                    viewCount = info.viewCount.takeIf { it >= 0 } ?: 0L,
                    uploadInstant = try {
                        info.uploadDate?.instant
                    } catch (_: Exception) {
                        null
                    },
                    description = try {
                        info.description?.content
                    } catch (_: Exception) {
                        null
                    } ?: "",
                    uploaderName = info.uploaderName ?: "",
                    uploaderAvatarUrl = avatars.maxByOrNull { it.height * it.width }?.url
                        ?: avatars.firstOrNull()?.url
                        ?: "",
                    uploaderSubs = info.uploaderSubscriberCount.takeIf { it >= 0 } ?: 0L,
                    uploaderUrl = info.uploaderUrl ?: "",
                    related = info.relatedStreams
                        .filterIsInstance<StreamInfoItem>()
                        .mapNotNull { item ->
                            // Reuse the feed mapper via a local copy: mapItem is
                            // private to VideoSearch, so map here identically.
                            val url = item.url ?: return@mapNotNull null
                            val id = url.substringAfter("v=", "")
                                .substringBefore("&")
                                .ifEmpty {
                                    url.substringAfter("/shorts/", "")
                                        .substringBefore("?").substringBefore("&").substringBefore("/")
                                }
                                .ifEmpty { return@mapNotNull null }
                            val thumbs = item.thumbnails
                            VideoSearchItem(
                                videoId = id,
                                title = item.name,
                                channel = item.uploaderName ?: "",
                                durationSecs = item.duration.takeIf { it >= 0 } ?: 0L,
                                viewCount = item.viewCount.takeIf { it >= 0 } ?: 0L,
                                thumbnailUrl = thumbs.maxByOrNull { it.height * it.width }?.url
                                    ?: thumbs.firstOrNull()?.url
                                    ?: "",
                                channelAvatarUrl = item.uploaderAvatars
                                    .maxByOrNull { it.height * it.width }?.url
                                    ?: item.uploaderAvatars.firstOrNull()?.url
                                    ?: "",
                                uploadDate = item.textualUploadDate ?: "",
                                isLive = item.streamType == StreamType.LIVE_STREAM,
                            )
                        },
                )
            }
        }

    suspend fun videoComments(videoId: String): Result<List<VideoComment>> =
        withContext(Dispatchers.IO) {
            runCatching {
                NewPipeExtractor.init()
                val service = NewPipe.getService(0)
                val handler = service.commentsLHFactory
                    .fromUrl("https://www.youtube.com/watch?v=$videoId")
                val extractor = service.getCommentsExtractor(handler)
                extractor.fetchPage()
                extractor.initialPage.items
                    .filterIsInstance<org.schabi.newpipe.extractor.comments.CommentsInfoItem>()
                    .mapNotNull { item ->
                        val text = try {
                            item.commentText?.content
                        } catch (_: Exception) {
                            null
                        } ?: ""
                        if (text.isEmpty()) return@mapNotNull null
                        val avatars = item.uploaderAvatars
                        VideoComment(
                            author = item.uploaderName ?: "",
                            avatarUrl = avatars.maxByOrNull { it.height * it.width }?.url
                                ?: avatars.firstOrNull()?.url
                                ?: "",
                            text = text,
                            likeCount = item.likeCount.takeIf { it >= 0 } ?: 0,
                            timeAgo = item.textualUploadDate ?: "",
                        )
                    }
            }
        }

    /** Resolve a UC channel id from an uploader URL (or fail gracefully). */
    suspend fun resolveChannelId(uploaderUrl: String): Result<String> =
        withContext(Dispatchers.IO) {
            runCatching {
                val direct = uploaderUrl.substringAfter("/channel/", "")
                    .substringBefore("/").substringBefore("?").substringBefore("&")
                if (direct.startsWith("UC")) return@runCatching direct
                NewPipeExtractor.init()
                val service = NewPipe.getService(0)
                val info = org.schabi.newpipe.extractor.channel.ChannelInfo.getInfo(
                    service,
                    uploaderUrl,
                )
                val canonical = info.url ?: ""
                val id = canonical.substringAfter("/channel/", "")
                    .substringBefore("/").substringBefore("?").substringBefore("&")
                if (id.startsWith("UC")) id
                else throw IllegalStateException("No channel id for $uploaderUrl")
            }
        }
}
