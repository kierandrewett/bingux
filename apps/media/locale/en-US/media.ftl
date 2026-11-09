### Bingux Media.
### Source language: American English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `media-`.

## Empty states and status

# Shown when the library has nothing in it.
media-empty-title = No music or videos
# Under the empty title, on how to add files.
media-empty-body = Add files to your Music or Videos folder, or drag them into this window.
# Button that opens the folder chooser from the empty library.
media-empty-action = Add folder…
media-empty-section-songs = No songs
media-empty-section-songs-body = Songs in your Music folder appear here.
media-empty-section-albums = No albums
media-empty-section-albums-body = Albums in your Music folder appear here.
media-empty-section-artists = No artists
media-empty-section-artists-body = Artists in your Music folder appear here.
media-empty-section-videos = No videos
media-empty-section-videos-body = Videos in your Videos folder appear here.
media-empty-queue = Nothing is queued
media-empty-queue-body = Songs and videos you play appear here.
media-no-results = No results
media-no-results-body = Check the spelling or try another word.
# Shown while the library is being rescanned.
media-scanning = Updating library…
media-not-playing = Not playing
media-cannot-play = Could not play this file. Try another copy of it.
media-frame-saved = Frame saved to Pictures
media-frame-not-saved = Could not save the frame. Check that Pictures has room.
# Shown over the window while files are dragged onto it.
media-drop-here = Drop to play

## Actions on songs, albums and videos

media-play = Play
media-shuffle = Play shuffled
media-play-next = Play next
media-play-later = Play later
media-remove-from-queue = Remove from queue
media-clear-up-next = Clear up next
media-show-album = Go to album
media-show-artist = Go to artist
media-continue-watching = Continue watching
media-all-videos = All videos
media-now-playing = Now playing
media-up-next = Up next
# Section headings in the sidebar and the library.
media-songs-heading = Songs
media-albums-heading = Albums
media-videos-heading = Videos
media-column-title = Title
media-column-artist = Artist
media-column-album = Album
media-column-time = Time

## Menus and commands

media-menu-open = Open file…
media-menu-add-folder = Add folder…
media-menu-scan = Update library
media-menu-settings = Settings
media-menu-mini = Mini player
media-menu-player = Show player
media-menu-full-screen = Full screen
media-menu-up-next = Go to up next
# The group the playback commands sit under in the command palette.
media-controls-group = Controls
media-command-play = Play
media-command-pause = Pause
media-command-next = Next
media-command-previous = Previous
media-command-louder = Increase volume
media-command-quieter = Decrease volume
media-command-mute = Mute
media-command-shuffle = Shuffle
media-command-repeat = Repeat
media-command-repeat-one = Repeat one

## Keyboard hints in the player

media-hint-play = Play or pause, in the player
media-hint-seek = Back or forward 5 seconds
media-hint-seek-long = Back or forward 30 seconds
media-hint-skip = Back or forward 10 seconds
media-hint-volume = Volume, in the player
media-hint-mute = Mute, in the player
media-hint-full-screen = Full screen, in the player
media-hint-speed = Slower or faster
media-hint-subtitles = Next subtitles
media-hint-frame = A frame back or forward, paused
media-hint-loop = Loop a part: start, end, off
media-hint-snapshot = Save the frame to Pictures
media-hint-leave = Leave full screen, then the player

## The player

media-close-player = Close player (Esc)
media-full-screen = Full screen (F)
media-leave-full-screen = Leave full screen (F)
media-mini-player = Mini player
media-leave-mini-player = Leave mini player
media-subtitles = Subtitles (C)
media-audio-track = Audio
# The speed menu's label, with the keys that change speed.
media-speed-menu = Speed ([ and ])
media-sleep-timer = Sleep timer
media-queue = Up next
media-off = Off
media-end-of-item = When this ends
media-stops-at-end = Stops when this ends
media-add-subtitle-file = Add subtitle file…
media-chapters = Chapters

# $time (String) - what is left of a film, already formatted, e.g. "1 hr, 3 min".
media-left = { $time } left
# $time (String) - a clock reading, e.g. 12:30.
media-stops-in = Stops in { $time }
# $title (String) - the song's title.
media-now-playing-spoken-alone = Now playing { $title }
# $title (String) - the song's title; $artist (String) - the song's artist.
media-now-playing-spoken = Now playing { $title } by { $artist }

## Counts

# $count (Number) - a number of songs. "1 song", "12 songs".
media-songs = { $count ->
    [one] 1 song
   *[other] { $count } songs
}
# $count (Number) - a number of albums. "1 album", "3 albums".
media-albums = { $count ->
    [one] 1 album
   *[other] { $count } albums
}
# $count (Number) - a number of artists. "1 artist", "4 artists".
media-artists = { $count ->
    [one] 1 artist
   *[other] { $count } artists
}
# $count (Number) - a number of videos. "1 video", "9 videos".
media-videos = { $count ->
    [one] 1 video
   *[other] { $count } videos
}

## Settings

media-settings-title = Settings
media-library-group = Library
media-playback-group = Playback
media-lastfm-group = Last.fm
media-remove-folder = Remove from library
media-levelling = Even out loudness
media-levelling-off = Off
media-levelling-track = Song by song
media-levelling-album = Album by album
media-levelling-auto = Automatic
media-resume = Resume videos
media-resume-body = Videos start where you left them.
media-subtitles-setting = Show subtitles
media-scrobble = Scrobble to Last.fm
media-api-key = API key
media-api-secret = Shared secret
media-api-help = Create an API account on Last.fm, then enter its key and secret.
media-sign-in = Sign in…
media-finish-sign-in = Finish signing in
media-sign-out = Sign out
media-signed-in-as = Signed in as
media-lastfm-unreachable = Could not reach Last.fm. Check your connection, then try again.
media-lastfm-bad-keys = Last.fm did not accept the key or secret. Check them, then try again.
media-lastfm-not-allowed = Allow Media on the Last.fm page, then try again.
media-lastfm-failed = Could not sign in to Last.fm. Try again later.
