# jellyfin-tui

The goal of this project is a CMUS-like streaming client for Jellyfin with a featureful TUI.

Currently most of the basic features are implemented. I'm struggling with ratatui's wonky layout system. I'll see if I can get it to work properly, otherwise I'll switch to another tui library.

The player has a cover image in the corner, this requires the use of a terminal that supports sixel graphics. I will add a fallback for terminals that don't support it in the future. You can find out if your terminal supports sixel [here](https://www.arewesixelyet.com).

I'm enjoying the development of this project, so I'll continue to work on it. I'm open to suggestions and feature requests.

### Installation
```bash
git clone
cd jellyfin-tui
cargo build --release
./target/release/jellyfin-tui
```

![image](screen.png)

### Key bindings
|key (alternative)|action|
|---|---|
|Space|play / pause|
|Down (J)|Navigate down|
|Up (K) |Navigate up|
|Right (S)|seek +5s|
|Left (R)|seek -5s|
|N|Next track|
|P|Previous track|
|Tab|Cycle between Artist & Track|
|Shift + Tab|Focus queue|
|Q|Quit|
