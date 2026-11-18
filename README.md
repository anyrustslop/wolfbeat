<p align="center"><img src="assets/logo.png" width="128" alt="Wolfbeat logo"></p>

# Wolfbeat

A fast, good-looking desktop music player for Windows, **written in Rust**. Play your own music files and search and play songs online, in one app.

> [!NOTE]
> Wolfbeat is an early, hobby project. Online search and playback work by reading Spotify, YouTube Music and YouTube web pages, so they can break whenever those sites change.

## Download
Get **Wolfbeat-&lt;version&gt;-win64.exe** from the [latest release](../../releases/latest) and run it (Windows 10/11, no install needed; the exe isn't code-signed, so SmartScreen may ask you to click **More info → Run anyway**). Each release also has `SHA256SUMS.txt` to verify the download, and the full source code.

Online playback uses Microsoft Edge WebView2, which is already installed on Windows 10/11.

## Features
- **Your music:** add folders and Wolfbeat plays MP3, FLAC, OGG, WAV, M4A and AAC with album art.
- **Online search:** one search box for Spotify, YouTube Music and YouTube results; play them right away.
- **Import Spotify playlists and albums** by pasting their link.
- **Liked songs, playlists and a queue**, shuffle and repeat.
- **Synced lyrics** (from LRCLIB) and a "Made for you" mix of similar songs.
- **Glass interface** with animated backgrounds, Windows 11 rounded window, and play / pause / next buttons in the taskbar preview.
- **Light on memory:** the online player is a single hidden page that closes when it isn't needed.

## Build
`cargo build --release` (Rust, MSVC + Windows SDK). The logo is drawn in code ([src/logo.rs](src/logo.rs)) and embedded as the exe icon by `build.rs`.

## License
MIT. See [LICENSE](LICENSE).
