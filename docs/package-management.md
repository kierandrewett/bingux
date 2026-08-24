# Package management

Bingux keeps permanent software choices in a profile. A profile is the source
of truth for its packages, Flatpaks, user applications, and desktop choices.

## Inventory is audit input

`bingux-inventory` is a read-only audit tool. Its canonical JSON reports
observed package-manager state; it does not establish why a package is
installed, which profile should own it, or which Flatpak remote supplied it.
Treat inventory output as review input only. Do not generate
`bingux.packages.*` or Flatpak declarations from it: doing so could turn
unmanaged or transient software into permanent profile state. Safe declaration
generation requires provenance, such as explicit ownership and source/origin
metadata, in addition to package name and version.

## Permanent packages

Use `bingux.packages.system` when a program is needed by system services,
administrators, or every user. Use `bingux.packages.user` for a profile user's
desktop and command-line applications.

For example, add a package to `profiles/kieran/default.nix`:

```nix
bingux.packages.user = with pkgs; [
    helix
];
```

Then apply the selected host configuration:

```sh
sudo nixos-rebuild switch --flake .#<host>
```

Remove the declaration and run the same command to remove the package from the
next generation. The current repository exposes installer and VM test hosts.
Add a real hardware host before you use `nixos-rebuild switch` on a workstation.

## Declarative Flatpaks

Flatpak remotes and applications belong in the selected profile. The Kieran
profile keeps them in `profiles/kieran/flatpaks.nix` and imports that file into
`bingux.packages.flatpaks`.

Add a standard Flathub application:

```nix
apps = [
    { appId = "org.example.Application"; }
];
```

Add an application from a declared non-default remote:

```nix
apps = [
    {
        appId = "org.example.Application";
        origin = "example-remote";
    }
];
```

Do not use an undeclared remote. If a profile declares any remotes, it must also
include every remote that its application declarations need. Bingux does not
remove Flatpaks that are not declared by the profile.

## Temporary packages

Use a temporary shell for a one-off command:

```sh
nix shell nixpkgs#jq
jq --version
exit
```

Run one command without changing the user environment:

```sh
nix run nixpkgs#hello
```

Use an imperative user profile only when a temporary experiment must survive a
logout or restart:

```sh
nix profile install nixpkgs#jq
nix profile list
nix profile remove <index>
```

An imperative Nix profile is not tracked by Bingux. Move a package into the
profile declaration when it becomes part of the workstation configuration.

## Package lookup

Search the pinned Nixpkgs input before you add a package:

```sh
nix search --inputs-from . nixpkgs <name>
```

Use the exact package attribute in a profile. Do not add shell download scripts,
manual `curl | sh` installers, or copied binaries when the package exists in
Nixpkgs. Keep software that must remain outside Nixpkgs in an explicit Flatpak
declaration or a documented profile-local integration.
