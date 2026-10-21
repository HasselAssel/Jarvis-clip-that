# Jarvis-clip-that

Jarvis-clip-that is a desktop clipping tool, with the main goal of separating the different audio tracks on the desktop.
The resulting clip file contains multiple audio tracks so one can cut out unwanted noise for maximum larp.

Currently Supported:
- Windows:
  - WasAPI Audio:
    - System Audio
    - Per Process Audio
    - Default Input Device
  - D3D11 Desktop Duplication
- Linux:
  - in development :(
- Minimal clip "editor" to edit and export clips:
  - in development :(

The config.toml must be in `config/config.toml` relative to the .exe right now.

Example `config.toml`:
```toml file="config.toml"
retention_secs = 60
output_dir = "C:\\Jarvis-Clips\\clips"

watch_processes = ["aac", true]

[keybinds]
save_clip = [
  ["Alt", "KeyM"]
]
save_clip_sound_file = "sounds\\BOOM.mp3"
save_clip_sound_volume = 0.05

[[recorders]]
name = "Main Screen: 0"

[recorders.frame_config]
type = "video"

[recorders.frame_config.config]
fps = 30
width = 2560
height = 1440
codec = "hevc_amf"

[recorders.frame_config.config.environment]
type = "d3_d11"
monitor = 0


[[recorders]]
name = "System Audio"

[recorders.frame_config]
type = "audio"

[recorders.frame_config.config]
codec = "aac"

[recorders.frame_config.config.environment]
type = "was_api"

[recorders.frame_config.config.environment.config]
type = "was_api_default_sys"

[[recorders]]
name = "Default Input"

[recorders.frame_config]
type = "audio"

[recorders.frame_config.config]
codec = "aac"

[recorders.frame_config.config.environment]
type = "was_api"

[recorders.frame_config.config.environment.config]
type = "was_api_default_input"
```