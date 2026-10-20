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
- Minimal Clip "Editor" to edit and export clips:
  - - in development :(

Example `config.toml`:
```toml file="config.toml"
retention_secs = 120
output_dir = "out"

# Have the program automatically scan for new processes that produce audio.
watch_processes = ["aac", true] 

[keybinds]
save_clip = [
    ["Alt", "KeyM"]
]

[[recorders]]
name = "Main Screen: 0"

[recorders.frame_config]
type = "video"

[recorders.frame_config.config]
fps = 30
# In progress: have the program figure out the width and height.
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