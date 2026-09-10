# Extensions

The public interface supports ordinary QML components. It does not restrict extensions to a fixed set of controls.
API versions describe the host contract, not the Bingux release. Extra manifest fields are allowed.
Extensions run as trusted code with the user's session access.

- [x] Discover local and system extension folders with per-extension errors.
- [x] Load widget components through the existing desktop layout and preview system.
- [x] Provide a lifecycle entry point and optional access to shell objects.
- [x] Preserve placements when an extension is disabled, missing or incompatible.
- [x] Test discovery, layout validation, previews, loading and unloading.
- [ ] Build Home Assistant as a separate extension and verify a real connection.

Use `~/.local/share/bingux/extensions/<id>/extension.json` for user extensions.
Use `~/.config/bingux/extensions.json` for enable state. Extension settings belong to each extension.
The loader must not modify the desktop layout or enable an extension merely because it was discovered.

Home Assistant connection and entity selection are deferred at the user's request.
