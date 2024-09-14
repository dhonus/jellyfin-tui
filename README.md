# jellyfin-tui

The goal of this project is a CMUS-like streaming client for Jellyfin with a featureful TUI.

The player has a cover image in the corner, courtesy of the [ratatui-image](https://github.com/benjajaja/ratatui-image) crate. Most modern terminals should support sixel graphics or equivalent.

I'm enjoying the development of this project, so I'll continue to work on it. I'm open to suggestions and feature requests.

### Features
- streams your music from Jellyfin
- global search
- last.fm scrobbling
- vim keybindings
- sixel cover image
- lyrics (from jellyfin 10.9)
- MPRIS controls
- queue

### Planned features
- playlists (play/create/edit)
- jellyfin-wide remote control
- transcoding
- general overview with recommended albums and favorites

### Screenshots
![image](screen.png)

### Installation
Jellyfin-tui uses libmpv as the backend for audio playback. You need to have mpv installed on your system.

#### Linux
Linux is the main target OS for this project. You can install mpv from your package manager.
```bash
# add ~/.cargo/bin to your PATH (~/.bashrc etc.) if you haven't already
export PATH=$PATH:~/.cargo/bin/

# install mpv
sudo pacman -S mpv # arch
sudo apt install mpv libmpv-dev # ubuntu

# clone and run
git clone https://github.com/dhonus/jellyfin-tui
cd jellyfin-tui
cargo run --release

# or install
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

### Configuration
When you run jellyfin-tui for the first time, it will ask you for the server address, username and password and save them in the configuration file.

The configuration file is located at `~/.config/jellyfin-tui/config.yaml`.
```yaml
server: "http://localhost:8096"
password: "password"
username: "username"
```

### Key bindings
|key / alt|action|
|---|---|
|space|play / pause|
|F1 - Library, F2 - Search|switch tab|
|F1, ESC|return to Library|
|down / j|navigate down|
|up / k|navigate up|
|right / s|skip +5s|
|left / r|skip -5s|
|n|next track|
|N|previous track; if over, 5s plays current track from the start|
|tab|cycle between Artist & Track|
|shift + tab|cycle between previous and Lyrics / Queue|
|+|volume up|
|-|volume down|
|q, ^C|quit|


### Search
You can search globally by pressing `F2` or `/`. The search is case insensitive and will search for artists, albums and tracks. It will pull **everything** without pagination, so it may take a while to load if you have a large library. This was done because jellyfin won't allow me to search for tracks without an artist or album assigned, which this client doesn't support.

![image](search.png)