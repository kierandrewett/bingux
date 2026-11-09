# Media

Media (`bingux-media`) plays the music and videos in your Music and Videos
folders. There is nothing to set up: open it and the library is there.

## The library

| Page    | What it shows                                                          |
| ------- | ---------------------------------------------------------------------- |
| Songs   | Every song by title, with Play and Shuffle                             |
| Albums  | A grid of albums; an album lists its tracks in disc and track order    |
| Artists | Artists by name; an artist shows their albums, newest first            |
| Videos  | Videos left part way first (Continue watching), then all, newest first |
| Up next | What is playing and what plays after it                                |

- **Where it looks.** The XDG Music and Videos folders (`~/Music`, `~/Videos`,
  or what `~/.config/user-dirs.dirs` names), plus folders added with
  **Add Folder…** in the main menu or Settings. Hidden folders are skipped.
- **Keeping up.** Media checks the folders each time it starts and with
  **Update Library**. Only new and changed files are read again, so a large
  library that has not changed takes a moment.
- **Tags and covers.** Title, artist, album, album artist, track and disc
  numbers, year and genre come from the file's tags. The cover is the picture
  inside the file, else a `cover.jpg`, `folder.jpg`, `front.png` or
  `AlbumArt*.jpg` beside it. A file with no tags is named after the file
  (`01 First Light.flac` is track 1, "First Light").
- **Search.** Type in the sidebar's search field (Ctrl+F). Every word must
  match a title, artist, album or genre.
- **Nothing found.** The window says so and offers **Add Folder…**. Files and
  folders can also be dragged into the window, or opened from Files with
  **Open With**: Media handles audio and video types.

## Playing

- Press a song to play it and the songs after it; press a video to play it
  full window. **Play next** and **Play later** (a song's context menu) add to
  the queue. Up next can move, remove and clear what is queued.
- The bar along the bottom has the artwork, the controls, the timeline and the
  volume. Press the artwork or title to open the full player. A video keeps
  playing in the bar when its player is closed.
- **Shuffle** mixes what is still to play; turning it off puts the queue back
  in its order. **Repeat** goes from off to all to one.
- Albums play **without gaps** between tracks.
- **Videos start where you left them.** A video stopped in its first or last
  15 seconds starts from the beginning next time.
- **Subtitles** inside the video, or in a file next to it with the same name
  (`Film.srt`, `Film.en.ass`), are found by themselves. The subtitles menu
  turns them off, chooses another and adds a file. Videos with several audio
  tracks or chapters have menus for them; chapters also show as gaps in the
  timeline.
- Hovering the timeline of a video shows that moment's frame.
- **Sleep timer** (the moon in the full player): stops after 15 to 60 minutes
  or at the end of the song or video, fading out over the last seconds.
- **Mini player**: a small window that keeps playing (the main menu, the bar,
  or the button in the full player).

## Keys

| Where       | Keys                                                                                                                                                                                                                                                                                      |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Anywhere    | Play/pause, next and previous media keys; Ctrl+Space play or pause; Ctrl+Left and Ctrl+Right previous and next; Ctrl+Up and Ctrl+Down volume; Ctrl+O open a file; Ctrl+, Settings                                                                                                         |
| Full player | Space or K play or pause; Left and Right 5 s (Shift: 30 s); J and L 10 s; Up and Down volume; M mute; F full screen; `[` and `]` slower and faster; C next subtitles; `,` and `.` a frame back and forward while paused; Escape leaves full screen, then the mini player, then the player |
| Timeline    | Left and Right 5 s (Shift: 30 s), Page Up and Page Down a minute, Home and End, `,` and `.` the chapter before and after                                                                                                                                                                  |

Double-click a video for full screen. The pointer and the controls hide while a
video plays and come back when the pointer moves. The wheel over the volume
changes it in steps of 5.

## Settings

| Setting             | Effect                                                                                                                                                                                                                                             |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Library folders     | The folders Media looks in. Removing Music or Videos takes it out of the library; the files stay where they are                                                                                                                                    |
| Even out loudness   | **Song by song** (default) plays songs from different albums at one level using their ReplayGain tags; **Album by album** keeps an album's quiet and loud tracks; **Automatic** evens the level as it plays, for files without ReplayGain; **Off** |
| Resume videos       | Videos start where you left them (on by default)                                                                                                                                                                                                   |
| Show subtitles      | Subtitles in your language are shown when a video has them (on by default)                                                                                                                                                                         |
| Scrobble to Last.fm | Off by default. See below                                                                                                                                                                                                                          |

### Last.fm

Media has no Last.fm key of its own. To scrobble:

1. Create an API account at <https://www.last.fm/api/account/create>.
2. In Settings, turn on **Scrobble to Last.fm** and enter the API key and
   shared secret.
3. Press **Sign In…**, allow Media on the Last.fm page that opens, then press
   **Finish Signing In**.

A song counts once half of it, or four minutes, has played. Plays made offline
wait and are sent later. The key, secret and session are kept in
`~/.config/bingux-media/lastfm.json`, readable only by you. **Sign Out**
forgets the session.

## The desktop's media controls

Media is on the session bus as `org.mpris.MediaPlayer2.bingux_media` (MPRIS 2),
so the shell's media controls, media keys handled by the desktop, `playerctl`
and phone remotes can play, pause, skip, seek, change the volume, shuffle and
repeat, and show the title, artist and cover.

```sh
playerctl --player=bingux_media metadata
playerctl --player=bingux_media play-pause
```

## Files

| Path                                     | What                                                               |
| ---------------------------------------- | ------------------------------------------------------------------ |
| `~/.config/bingux-media/settings.json`   | Settings                                                           |
| `~/.config/bingux-media/lastfm.json`     | Last.fm key, secret and session                                    |
| `~/.local/share/bingux-media/library.db` | The library index, resume points, play counts and unsent scrobbles |
| `~/.cache/bingux-media/artwork/`         | Covers taken out of songs, one per album                           |
| `~/.cache/thumbnails/large/`             | Video thumbnails, shared with Files and other programs             |

## Requirements and troubleshooting

- **libmpv** (mpv 0.35 or later) plays everything: `libmpv2` on Debian and
  Ubuntu, `mpv-libs` on Fedora, `mpv` on Arch. Building needs `libmpv-dev`.
- **Sound** goes to PipeWire or PulseAudio, else ALSA. `BINGUX_MEDIA_AO=null`
  plays silently (for tests).
- **Video drawing.** Media draws video with mpv's GPU renderer in an
  off-screen OpenGL context, and with mpv's software renderer when there is no
  GPU driver (a virtual machine). `BINGUX_MEDIA_RENDER=sw` or `gl` forces one.
- **A file does not play.** The player says so. `BINGUX_MEDIA_MPV_LOG=v
bingux-media FILE` prints mpv's log.
