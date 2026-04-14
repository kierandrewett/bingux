# Package Compatibility Notes

## Edge Case Registry

### PATCHELF
| # | Package | Issue | Resolution | Commit |
|---|---------|-------|------------|--------|
| | | | | |

### SANDBOX
| # | Package | Issue | Resolution | Commit |
|---|---------|-------|------------|--------|
| | | | | |

### PERMISSIONS
| # | Package | Issue | Resolution | Commit |
|---|---------|-------|------------|--------|
| | | | | |

### BUILD SYSTEM
| # | Package | Issue | Resolution | Commit |
|---|---------|-------|------------|--------|
| | | | | |

## Known Issues by Package Type

### Rust binaries (ripgrep, fd, bat)
- Statically linked with musl: no patchelf needed for deps, but PT_INTERP still needs rewriting
- cargo needs network during fetch phase of build container

### Python/Node interpreters
- Shebangs in installed scripts need rewriting
- .pyc files may embed host paths
- pip/npm install at runtime writes to per-package home

### Daemons (nginx, postgresql)
- Need pre-granted permissions in system.toml (no GUI prompt for headless services)
- Data directories in /system/state/<pkg>/
- Socket files in state dir, not /var/run

### Electron apps (code-oss)
- Bundle their own Chromium — patchelf the bundled binary
- Native Node modules need patchelf
- Sandbox-within-sandbox considerations

### GPU-dependent apps (firefox, mpv, blender)
- dlopen for VA-API, VDPAU, PipeWire, CUDA/HIP
- Need dlopen_hints or LD_LIBRARY_PATH in sandbox
- GPU permission must be granted for hardware acceleration

### Large build systems (libreoffice, firefox)
- Build time >1 hour — need configurable build timeout
- Complex configure scripts may not respect DESTDIR properly
