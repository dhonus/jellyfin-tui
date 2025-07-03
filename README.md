# jellyfin-tui

Jellyfin-tui is a (music) streaming client for the Jellyfin media server. Inspired by CMUS and others,
its goal is to offer a self-hosted, terminal music player with all the modern features you need.

Most modern music players are either primarily electron based memory hogs that sell your personal data, too platform-specific, or *are* tui-based, but way too minimalistic and a big pain to set up. The streaming aspect of jellyfin-tui also solves the problem of duplicating and synchronizing your library across multiple devices, my biggest gripe with using a "normal" music player.

### Features
- stream your music from Jellyfin
- lyrics with autoscroll (Jellyfin > 10.9)
- sixel **cover image**, courtesy of [ratatui-image](https://github.com/benjajaja/ratatui-image)
- transcoding
- spotify-like double queue with order control, etc.
- metadata caching, downloading and a fully **offline mode**
- last.fm scrobbling, you need [jellyfin-plugin-lastfm](https://github.com/jesseward/jellyfin-plugin-lastfm)
- vim keybindings
- MPRIS controls
- playlists (play/create/edit)
- works over ssh
- fast and just kind of nifty really

### Planned features
- other media types (movies, tv shows)
- jellyfin-wide remote control and much more
- if there is a feature you'd like to see, please open an issue :)

### Screenshots
![image](.github/optimized.gif)

### Installation
Jellyfin-tui uses libmpv as the backend for audio playback. You need to have mpv installed on your system.

#### Arch Linux
[jellyfin-tui](https://aur.archlinux.org/packages/jellyfin-tui/) is available as a package in the [AUR](https://aur.archlinux.org). You can install it with your preferred [AUR helper](https://wiki.archlinux.org/title/AUR_helpers). Example:
```bash
paru -S jellyfin-tui
```

#### Nix
[jellyfin-tui](https://search.nixos.org/packages?channel=unstable&show=jellyfin-tui&from=0&size=50&sort=relevance&type=packages&query=jellyfin-tui) is available as a package in [Nixpkgs](https://search.nixos.org/packages).

#### Other Linux
Linux is the main target OS for this project. You can install mpv from your package manager.
```bash
# add ~/.cargo/bin to your PATH (~/.bashrc etc.) if you haven't already
export PATH=$PATH:~/.cargo/bin/

# install mpv
sudo pacman -S mpv sqlite # arch
sudo apt install mpv libmpv-dev sqlite3 libssl-dev # ubuntu
```
```bash
# clone this repository
git clone https://github.com/dhonus/jellyfin-tui
cd jellyfin-tui

# checkout the latest stable version if desired
# (git pull and re-run to update)
git checkout $(git tag | tail -1)

cargo run --release

# or install it system-wide to run `jellyfin-tui` anywhere
export PATH=$PATH:~/.cargo/bin/
cargo install --path .
```

#### macOS
```bash
brew install mpv
git clone https://github.com/dhonus/jellyfin-tui
cd jellyfin-tui
export LIBRARY_PATH="$LIBRARY_PATH:$(brew --prefix)/lib"
export PATH=$PATH:~/.cargo/bin/
cargo install --path .
```
### Key bindings
Press **`?`** to see the key bindings at any time. Some of the most important ones are:

<details>
<summary>Key bindings</summary>
<br>

|key|alt|action|
|---|---|---|
|space||play / pause|
|enter||start playing selected|
|up / down|k / j|navigate **up** / **down**|
|tab||cycle between **Artist** & **Track** lists|
|shift + tab||cycle further to **Lyrics** & **Queue**|
|p||show **command prompt**|
|a / A||skip to next / previous **album**, or next in Artists, alphabetically|
|1,2,3,...|F1,F2,F3,...|switch tab >> F1 - **Library**, F2 - **Search**|
|F1|ESC|return to **Library** tab|
|left / right|r / s|seek +/- 5s|
|. / ,|< / >|seek +/- 1m|
|d||download track / album / playlist|
|n||next track|
|N||previous track; if over 5s plays current track from the start|
|+ -||volume up / down|
|ctrl + e|ctrl + enter|play next|
|e|shift + enter|enqueue (play last)|
|E||clear queue|
|DELETE||remove from queue|
|x||stop playback|
|X||reset the program|
|T||toggle transcode (applies to newly added songs, not whole queue)|
|q|^C|quit|

</details>

### Popup
There are only so many keys to bind, so some actions are hidden behind a popup. Press `p` to open it and `ESC` to close it. The popup is context sensitive and will show different options depending on where you are in the program.

![image](.github/popup.png)

### Queue
Jellyfin-tui has a double queue similar to Spotify. You can add songs to the queue by pressing `e` or `shift + enter`. Learn more about what you can do with the queue by pressing `?` and reading through the key bindings.

![image](.github/queue.png)

### Configuration
When you run jellyfin-tui for the first time, it will ask you for the server address, username and password and save them in the configuration file.

The program **prints the config location** when run. On linux, the configuration file is located at `~/.config/jellyfin-tui/config.yaml`. Feel free to edit it manually if needed.
```yaml
#= You can define multiple servers here
servers:
  - name: Main Server
    url: 'https://jellyfin.example.com'
    username: 'username'
    password: 'imcool123'
    default: true # Add to not ask to pick server. Use --select-server to override
  - name: Second Server
    url: 'http://localhost:8096'
    username: 'username'
    password: 'imcool123'

# All following settings are OPTIONAL. What you see here are the defaults.

# Show album cover image
art: true
# Save and restore the state of the player (queue, volume, etc.)
persist: true
# Grab the primary color from the cover image (false => uses `primary_color` instead)
auto_color: true
# Hex or color name ('green', 'yellow' etc.). If not specified => blue is used.
primary_color: '#7db757'

transcoding:
  bitrate: 320
  # container: mp3

# Options specified here will be passed to mpv - https://mpv.io/manual/master/#options
mpv:
  af: lavfi=[loudnorm=I=-16:TP=-3:LRA=4]
  no-config: true
  log-file: /tmp/mpv.log
```

### MPRIS
Jellyfin-tui registers itself as an MPRIS client, so you can control it with any MPRIS controller. For example, `playerctl`.

### Search

In the Artists and Tracks lists you can search by pressing `/` and typing your query. The search is case insensitive and will filter the results as you type. Pressing `ESC` will clear the search and keep the current item selected.

You can search globally by switching to the Search tab. The search is case insensitive and will search for artists, albums and tracks. It will pull **everything** without pagination, so it may take a while to load if you have a large library. This was done because jellyfin won't allow me to search for tracks without an artist or album assigned, which this client doesn't support.

![image](.github/search.png)

### Downloading media / offline mode

Downloading music is very simple, just **press `d` on a track**, or album. More download options can be found in popups.

A local copy of commonly used data is stored in a local database. This speeds up load times and allows you to use the program fully offline. Also, playing a downloaded track will play the local copy instead of streaming it, saving bandwidth.
> Your library is updated **in the background** every 10 minutes. You will be notified if anything changes. Track metadata updates whenever you open a discography/album/playlist view in-place. You can also force an update in the global popup menu. Jellyfin is the parent, if you delete music on the server, jellyfin-tui will also delete it including downloaded files.

### Recommendations
Due to the nature of the project and jellyfin itself, there are some limitations and things to keep in mind:
- jellyfin-tui assumes you correctly tag your music files. Please look at the [jellyfin documentation](https://jellyfin.org/docs/general/server/media/music/) on how to tag your music files. Before assuming the program is broken, verify that they show up correctly in Jellyfin itself.
- if your **cover image** has a black area at the bottom, it is because it's not a perfect square. Please crop your images to a 1:1 aspect ratio for the best results.
- **lyrics**: jellyfin-tui will show lyrics if they are available in jellyfin. To scroll automatically with the song, they need to contain timestamps. I recommend using the [LrcLib Jellyfin plugin](https://github.com/jellyfin/jellyfin-plugin-lrclib) and running `Download missing lyrics` directly **within jellyfin-tui** (Global Popup > Run scheduled task > Library: Download missing lyrics), or alternatively the desktop application [LRCGET](https://github.com/tranxuanthang/lrcget), both by by tranxuanthang. If you value their work, consider donating to keep this amazing free service running.

### Supported terminals
Not all terminals have the features needed to cover every aspect of jellyfin-tui. While rare, some terminals lack sixel (or equivalent) support or have certain key event limitations. The following are tested and work well:
- kitty (recommended)
- iTerm2 (recommended)
- ghostty
- wezterm
- konsole

The following have issues
- alacritty, gnome console, terminator (no sixel support and occasional strange behavior)
